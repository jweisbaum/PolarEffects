//! Keeping the certificate catalogues current by themselves (spec.md 5.4):
//! when the ORC and ORR scrapes run without being asked for by hand.
//!
//! Each catalogue has a schedule in Settings: only when asked (the default),
//! when the application starts, or when it quits. A scheduled scrape is the
//! person's standing request, which is what lets it use the network at all
//! (invariant 4), and it is skipped when that catalogue was written less
//! than a day ago: a full ORC scrape is some sixty megabytes from a public
//! service, and nobody's certificates change by the hour.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use tauri::{Emitter, Manager};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::settings::Settings;

/// When a catalogue is scraped by itself: only when asked, or at startup or
/// shutdown once the person has chosen that (spec.md 5.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScrapeSchedule {
    /// Only when asked.
    #[default]
    OnDemand,
    /// When the application starts.
    Startup,
    /// When the application quits.
    Shutdown,
}

/// How long after a catalogue was written a scheduled scrape leaves it be.
pub const FRESH: Duration = Duration::from_secs(24 * 60 * 60);

/// Tells the frontend that quitting is waiting for a scheduled scrape, and
/// that asking to quit again stops it.
pub const QUIT_WAITING: &str = "app://quit-waiting";

/// When each catalogue is scraped without being asked for by hand.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "CatalogueSettings.ts")]
#[serde(default)]
pub struct CatalogueSettings {
    /// The ORC catalogue, from ORC's own service.
    pub orc_schedule: ScrapeSchedule,
    /// The ORR catalogue, from RegattaMan's valid list.
    pub orr_schedule: ScrapeSchedule,
}

/// When the file at `path` was last written, UTC epoch seconds.
pub fn written_at(path: &Path) -> Option<i64> {
    let written = std::fs::metadata(path).ok()?.modified().ok()?;
    let seconds = written
        .duration_since(SystemTime::UNIX_EPOCH)
        .ok()?
        .as_secs();
    i64::try_from(seconds).ok()
}

/// The current year, UTC: the certificate year a scheduled scrape asks for.
pub fn current_year() -> i32 {
    use chrono::Datelike;
    let seconds = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_secs());
    chrono::DateTime::from_timestamp(i64::try_from(seconds).unwrap_or(0), 0)
        .unwrap_or_default()
        .year()
}

/// Whether a catalogue kept at `path` is to be scraped at `moment`: its
/// schedule says so, and it was not written within the last day. A file
/// dated in the future (a clock set back) counts as just written.
pub fn due(schedule: ScrapeSchedule, moment: ScrapeSchedule, path: &Path, now: SystemTime) -> bool {
    if schedule != moment {
        return false;
    }
    let Some(written) = std::fs::metadata(path).ok().and_then(|m| m.modified().ok()) else {
        return true;
    };
    now.duration_since(written).is_ok_and(|age| age >= FRESH)
}

fn schedules(state: &AppState) -> CatalogueSettings {
    state
        .with_session(|session| Ok(session.settings.catalogues))
        .unwrap_or_default()
}

/// The scrapes that are due at `moment`: ORC, ORR.
fn due_now(state: &AppState, moment: ScrapeSchedule) -> (bool, bool) {
    let chosen = schedules(state);
    let now = SystemTime::now();
    (
        due(
            chosen.orc_schedule,
            moment,
            &crate::orc::cache_path(state),
            now,
        ),
        due(
            chosen.orr_schedule,
            moment,
            &crate::orr::cache_path(state),
            now,
        ),
    )
}

/// Starts the scrapes named. One that cannot start (it is already running)
/// is left to the one that is.
fn start<R: tauri::Runtime>(app: &tauri::AppHandle<R>, orc: bool, orr: bool) {
    if orc {
        let _ = crate::orc::start_orc_scrape(app.clone(), app.state());
    }
    if orr {
        let _ = crate::orr::start_orr_scrape(app.clone(), app.state(), None, current_year());
    }
}

/// The startup schedule. Off the setup thread: starting an ORC scrape
/// decodes the catalogue first, and the window should not wait for that.
pub fn on_startup<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    let (orc, orr) = due_now(&app.state::<AppState>(), ScrapeSchedule::Startup);
    if orc || orr {
        tauri::async_runtime::spawn_blocking(move || start(&app, orc, orr));
    }
}

/// Where a quit stands with the shutdown schedule.
#[derive(Debug, Default)]
pub struct Shutdown {
    started: AtomicBool,
    finished: AtomicBool,
}

/// The shutdown schedule: answers whether quitting has to wait.
///
/// The first time a quit reaches here with a scrape due, the scrape starts,
/// the frontend is told, and the quit is asked for again once the scrape
/// ends. A quit asked for while that scrape runs stops it — a scrape is
/// stored whole or not at all, so the catalogue is as it was — and the
/// application quits as soon as it has stopped. Nobody is held in a window
/// that will not close.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let state = app.state::<AppState>();
    let flags = &state.catalogue_shutdown;
    if flags.finished.load(Ordering::SeqCst) {
        return false;
    }
    if flags.started.load(Ordering::SeqCst) {
        let _ = crate::orc::cancel(&state);
        let _ = crate::orr::cancel(&state);
        return true;
    }
    let (orc, orr) = due_now(&state, ScrapeSchedule::Shutdown);
    if !orc && !orr {
        return false;
    }
    if flags.started.swap(true, Ordering::SeqCst) {
        return true;
    }
    let revision = state
        .with_session(|s| Ok(s.open.as_ref().map(|o| (o.project.id, o.revision))))
        .ok()
        .flatten();
    let _ = app.emit(QUIT_WAITING, ());
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        start(&app, orc, orr);
        let state = app.state::<AppState>();
        while crate::orc::scraping(&state) || crate::orr::scraping(&state) {
            std::thread::sleep(Duration::from_millis(100));
        }
        state
            .catalogue_shutdown
            .finished
            .store(true, Ordering::SeqCst);
        // The project was edited while the scrape ran: the unsaved-changes
        // guard has to be answered again.
        let current = state
            .with_session(|s| Ok(s.open.as_ref().map(|o| (o.project.id, o.revision))))
            .ok()
            .flatten();
        if current != revision {
            state.exit_allowed.store(false, Ordering::SeqCst);
        }
        crate::quit::request(&app);
    });
    true
}

/// Whether quitting has to wait for scheduled work: the track library's
/// scrape, then the catalogues'.
pub fn work_before_exit<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    crate::library::shutdown(app) || shutdown(app)
}

/// Chooses when a catalogue (`"orc"` or `"orr"`) is scraped by itself.
#[tauri::command]
pub fn set_catalogue_schedule(
    state: tauri::State<'_, AppState>,
    catalogue: String,
    schedule: ScrapeSchedule,
) -> Result<Settings> {
    schedule_set(&state, &catalogue, schedule)
}

/// [`set_catalogue_schedule`] without a Tauri handle.
pub fn schedule_set(
    state: &AppState,
    catalogue: &str,
    schedule: ScrapeSchedule,
) -> Result<Settings> {
    crate::settings::update(state, |settings| {
        match catalogue {
            "orc" => settings.catalogues.orc_schedule = schedule,
            "orr" => settings.catalogues.orr_schedule = schedule,
            other => {
                return Err(AppError::BadOption {
                    field: "Catalogue",
                    value: other.to_owned(),
                });
            }
        }
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pe-catalogues-{label}-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn a_scheduled_scrape_is_due_at_its_moment_and_at_most_once_a_day() {
        let dir = temp("due");
        let file = dir.join("catalogue.bin");
        let now = SystemTime::now();
        use ScrapeSchedule::{OnDemand, Shutdown, Startup};
        // Never scraped: due at its moment, and at no other.
        assert!(due(Startup, Startup, &file, now));
        assert!(due(Shutdown, Shutdown, &file, now));
        assert!(!due(Startup, Shutdown, &file, now));
        assert!(!due(Shutdown, Startup, &file, now));
        assert!(!due(OnDemand, Startup, &file, now));
        assert!(!due(OnDemand, Shutdown, &file, now));

        // Written just now: left alone for a day, to the second.
        std::fs::write(&file, b"x").unwrap();
        let written = std::fs::metadata(&file).unwrap().modified().unwrap();
        assert!(!due(Startup, Startup, &file, written));
        assert!(!due(
            Startup,
            Startup,
            &file,
            written + FRESH - Duration::from_secs(1)
        ));
        assert!(due(Startup, Startup, &file, written + FRESH));
        assert!(due(Shutdown, Shutdown, &file, written + FRESH * 3));
        // A clock set back does not make it due.
        assert!(!due(
            Startup,
            Startup,
            &file,
            written - Duration::from_secs(3600)
        ));
        assert_eq!(
            written_at(&file),
            i64::try_from(
                written
                    .duration_since(SystemTime::UNIX_EPOCH)
                    .unwrap()
                    .as_secs()
            )
            .ok()
        );
        assert_eq!(written_at(&dir.join("absent")), None);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn each_catalogue_has_its_own_schedule_and_it_is_saved() {
        let dir = temp("schedule");
        let state = AppState::new(crate::paths::AppPaths::in_directory(&dir).unwrap());
        assert_eq!(schedules(&state), CatalogueSettings::default());
        assert_eq!(schedules(&state).orc_schedule, ScrapeSchedule::OnDemand);

        let saved = schedule_set(&state, "orc", ScrapeSchedule::Startup).unwrap();
        assert_eq!(saved.catalogues.orc_schedule, ScrapeSchedule::Startup);
        assert_eq!(saved.catalogues.orr_schedule, ScrapeSchedule::OnDemand);
        schedule_set(&state, "orr", ScrapeSchedule::Shutdown).unwrap();
        assert_eq!(
            schedule_set(&state, "irc", ScrapeSchedule::Startup)
                .unwrap_err()
                .kind(),
            "bad-option"
        );

        // Another launch reads them back, each by itself.
        let again = AppState::new(crate::paths::AppPaths::in_directory(&dir).unwrap());
        assert_eq!(
            schedules(&again),
            CatalogueSettings {
                orc_schedule: ScrapeSchedule::Startup,
                orr_schedule: ScrapeSchedule::Shutdown,
            }
        );
        // Due by the schedule alone while nothing was ever scraped.
        assert_eq!(due_now(&again, ScrapeSchedule::Startup), (true, false));
        assert_eq!(due_now(&again, ScrapeSchedule::Shutdown), (false, true));
        // Once the ORC store is written, its startup scrape waits a day.
        std::fs::write(crate::orc::cache_path(&again), b"x").unwrap();
        assert_eq!(due_now(&again, ScrapeSchedule::Startup), (false, false));
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn one_unreadable_schedule_costs_only_itself() {
        let dir = temp("unreadable");
        let paths = crate::paths::AppPaths::in_directory(&dir).unwrap();
        std::fs::write(
            paths.settings_file(),
            r#"{"catalogues": {"orc_schedule": "hourly", "orr_schedule": "startup"}}"#,
        )
        .unwrap();
        let state = AppState::new(paths);
        assert_eq!(
            schedules(&state),
            CatalogueSettings {
                orc_schedule: ScrapeSchedule::OnDemand,
                orr_schedule: ScrapeSchedule::Startup,
            }
        );
        let _ = std::fs::remove_dir_all(dir);
    }
}
