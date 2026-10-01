use super::{DatabaseSettings, ScrapeSchedule, catalogue::Catalogue};
use crate::{
    commands::AppState,
    error::{AppError, Context, Result},
};
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    path::PathBuf,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::Manager;
use ts_rs::TS;

#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct DatabaseProgress {
    pub running: bool,
    pub operation: String,
    pub done: u32,
    pub total: u32,
    pub tracks: u32,
    pub skipped: u32,
    pub failed: u32,
    pub current: String,
    pub cancelled: bool,
    pub error: Option<String>,
    pub failures: Vec<String>,
}
#[derive(Debug, Default)]
pub(super) struct Store {
    pub progress: DatabaseProgress,
    pub cancel: Option<Arc<AtomicBool>>,
    pub catalogue: Option<(PathBuf, Arc<Catalogue>)>,
}
#[derive(Debug, Default)]
pub struct DatabaseState {
    store: Mutex<Store>,
    shutdown_started: AtomicBool,
    shutdown_finished: AtomicBool,
}
impl DatabaseState {
    pub fn load(path: &std::path::Path) -> Self {
        let mut progress: DatabaseProgress = std::fs::read(path)
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        progress.running = false;
        Self {
            store: Mutex::new(Store {
                progress,
                ..Default::default()
            }),
            ..Default::default()
        }
    }
    pub(super) fn lock(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| AppError::Internal("Database job lock was poisoned".into()))
    }
    pub(super) fn update(&self, f: impl FnOnce(&mut DatabaseProgress)) {
        if let Ok(mut s) = self.lock() {
            f(&mut s.progress);
        }
    }
}
#[tauri::command]
pub fn database_job_status(state: tauri::State<'_, AppState>) -> Result<DatabaseProgress> {
    Ok(state.database.lock()?.progress.clone())
}
#[tauri::command]
pub fn cancel_database_job(state: tauri::State<'_, AppState>) -> Result<()> {
    if let Some(c) = &state.database.lock()?.cancel {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}
#[tauri::command]
pub fn start_database_job(
    app: tauri::AppHandle,
    operation: String,
    path: Option<String>,
) -> Result<DatabaseProgress> {
    start(app, &operation, path)
}
fn start<R: tauri::Runtime>(
    app: tauri::AppHandle<R>,
    operation: &str,
    path: Option<String>,
) -> Result<DatabaseProgress> {
    if !["metadata", "scrape", "export"].contains(&operation) {
        return Err(AppError::Internal("Unknown database operation".into()));
    }
    let state = app.state::<AppState>();
    let settings = super::settings(&state)?;
    settings.validate()?;
    if operation == "scrape" && settings.geojson_directory.is_empty() {
        return Err(AppError::Internal(
            "Choose a GeoJSON directory before scraping".into(),
        ));
    }
    if operation == "export" && path.is_none() {
        return Err(AppError::Internal(
            "Choose where to export the database".into(),
        ));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = DatabaseProgress {
        running: true,
        operation: operation.into(),
        ..Default::default()
    };
    {
        let mut store = state.database.lock()?;
        if store.progress.running {
            return Err(AppError::Internal(
                "A database operation is already running".into(),
            ));
        }
        store.progress = progress.clone();
        store.cancel = Some(Arc::clone(&cancel));
    }
    let operation = operation.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = match operation.as_str() {
            "metadata" => super::catalogue::download(&state, &settings, &cancel)
                .map(|count| state.database.update(|s| s.tracks = count as u32)),
            "scrape" => scrape(&state, &settings, &cancel),
            "export" => {
                super::export::run(&settings, &PathBuf::from(path.unwrap_or_default()), &cancel)
            }
            _ => unreachable!(),
        };
        if operation == "scrape"
            && result.is_err()
            && state.database.lock().is_ok_and(|s| s.progress.tracks > 0)
        {
            // Completed races survive cancellation; publish their searchable metadata too.
            if let Err(e) = super::catalogue::download(&state, &settings, &AtomicBool::new(false)) {
                failure(
                    &state,
                    "Metadata refresh after interrupted scrape",
                    &e.to_string(),
                );
            }
        }
        if let Ok(mut store) = state.database.lock() {
            store.progress.running = false;
            store.progress.cancelled = result.is_err() && cancel.load(Ordering::SeqCst);
            if let Err(e) = result
                && !store.progress.cancelled
            {
                store.progress.error = Some(e.to_string());
            }
            store.cancel = None;
            if let Ok(bytes) = serde_json::to_vec(&store.progress) {
                let _ = pe_core::io::write_atomic(
                    &state.paths.config_dir.join("database-last-job.json"),
                    &bytes,
                );
            }
        }
    });
    Ok(progress)
}
fn scrape(state: &AppState, settings: &DatabaseSettings, cancel: &Arc<AtomicBool>) -> Result<()> {
    let timeout = state.with_session(|s| Ok(s.settings.network.timeout_s))?;
    let fetcher = pe_trackers::Fetcher::new(
        "SYRF",
        Duration::from_secs(u64::from(timeout)),
        Arc::clone(cancel),
    )?;
    let mut urls = BTreeMap::new();
    let mut yellowbrick_races = BTreeMap::new();
    let explicit: Vec<_> = settings
        .scrape_urls
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    let limited = !explicit.is_empty();
    if limited {
        for u in explicit {
            let e = pe_trackers::library::resolve(u)?;
            urls.insert(queue_key(&e), (u.to_owned(), e));
        }
    } else {
        for u in super::ingest::known_urls(settings)? {
            if let Ok(e) = pe_trackers::library::resolve(&u) {
                urls.insert(queue_key(&e), (u, e));
            }
        }
        for tracker in [
            pe_core::track::Tracker::YellowBrick,
            pe_core::track::Tracker::Geovoile,
            pe_core::track::Tracker::BlueWaterTracks,
        ] {
            super::check(cancel)?;
            state.database.update(|s| {
                s.current = format!("Discovering {}", pe_trackers::library::source(tracker))
            });
            let discovered: Result<Vec<String>> = if tracker == pe_core::track::Tracker::YellowBrick
            {
                discover_yellowbrick(state, settings, &fetcher, &mut yellowbrick_races)
            } else {
                pe_trackers::library::discover(&fetcher, tracker).map_err(Into::into)
            };
            match discovered {
                Ok(found) => {
                    if found.is_empty() {
                        state.database.update(|s|s.failures.push(format!("{}: no public race links were listed; using database URLs. Add new race URLs in Settings.",pe_trackers::library::source(tracker))));
                    }
                    for u in found {
                        if let Ok(e) = pe_trackers::library::resolve(&u) {
                            urls.entry(queue_key(&e)).or_insert((u, e));
                        }
                    }
                }
                Err(e) => failure(
                    state,
                    &format!("{} discovery", pe_trackers::library::source(tracker)),
                    &e.to_string(),
                ),
            }
        }
    }
    let mut queue: Vec<_> = urls.into_values().collect();
    let mut cursor = 0;
    while cursor < queue.len() {
        super::check(cancel)?;
        let (original, event) = queue[cursor].clone();
        state.database.update(|s| {
            s.current = original.clone();
            s.done = cursor as u32;
            s.total = queue.len() as u32;
        });
        let result: Result<()> = (|| {
            if !limited && super::ingest::complete(settings, &event)? {
                state.database.update(|s| s.skipped += 1);
                return Ok(());
            }
            let client = pe_trackers::event::client(event.tracker)
                .ok_or_else(|| AppError::Internal("Tracker unavailable".into()))?;
            let catalogue = yellowbrick_races
                .get(&queue_key(&event))
                .and_then(Option::as_ref);
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs() as i64);
            let fetched = client.fetch_for_scrape(&event, &fetcher, &mut |_| {}, now)?;
            use pe_trackers::library::completion::ScrapeFetch;
            let legs = match &fetched {
                ScrapeFetch::Finished(event) => event.leg,
                ScrapeFetch::Skipped { legs } => *legs,
            };
            if let Some((_, legs)) = legs {
                for leg in 1..=legs {
                    let site = pe_trackers::geovoile::site(&original)?;
                    let url = format!(
                        "{}?leg={leg}",
                        site.url().split('?').next().unwrap_or_default()
                    );
                    let e = client.resolve(&url)?;
                    if !queue
                        .iter()
                        .any(|(_, old)| old.key == e.key && old.tracker == e.tracker)
                    {
                        queue.push((url, e));
                    }
                }
            }
            let ScrapeFetch::Finished(mut event) = fetched else {
                state.database.update(|s| s.skipped += 1);
                return Ok(());
            };
            let geometries = pe_trackers::library::course(&fetcher, &mut event)?;
            let count =
                super::ingest::save(settings, &event, &original, &geometries, catalogue, cancel)?;
            state.database.update(|s| s.tracks += count as u32);
            Ok(())
        })();
        if let Err(e) = result {
            failure(state, &original, &format!("{e}"));
        }
        cursor += 1;
    }
    // Rebuild metadata even after per-race failures. Completed races remain discoverable.
    let saved = state.database.lock()?.progress.clone();
    super::catalogue::download(state, settings, cancel)?;
    state.database.update(|s| {
        s.done = cursor as u32;
        s.total = queue.len() as u32;
        s.tracks = saved.tracks;
        s.current = String::new();
    });
    Ok(())
}
fn queue_key(event: &pe_trackers::EventRef) -> String {
    let key = if event.tracker == pe_core::track::Tracker::YellowBrick {
        event.key.to_ascii_lowercase()
    } else {
        event.key.clone()
    };
    format!("{:?}/{key}", event.tracker)
}
fn discover_yellowbrick(
    state: &AppState,
    settings: &DatabaseSettings,
    fetcher: &pe_trackers::Fetcher,
    races: &mut BTreeMap<String, Option<pe_trackers::library::yellowbrick::Race>>,
) -> Result<Vec<String>> {
    use pe_trackers::library::yellowbrick::{Credentials, discover};
    let credentials = (!settings.yellowbrick_user_key.trim().is_empty()).then_some(Credentials {
        user_key: &settings.yellowbrick_user_key,
        device_id: &settings.yellowbrick_device_id,
    });
    let known = super::ingest::yellowbrick_codes(settings)?;
    let report = discover(fetcher, credentials.as_ref(), &known, &mut |message| {
        state.database.update(|s| s.current = message);
    })?;
    let path = settings
        .metadata_path(state)
        .with_file_name("yellowbrick-races.json");
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| AppError::Internal("Invalid metadata directory".into()))?,
    )
    .doing("create", "YellowBrick catalogue directory")?;
    let bytes = serde_json::to_vec_pretty(&report).doing("encode", "YellowBrick catalogue")?;
    fetcher.check()?;
    pe_core::io::write_atomic(&path, &bytes)?;
    for race in &report.races {
        for url in &race.urls {
            if let Ok(event) = pe_trackers::library::resolve_source("YELLOWBRICK", url) {
                races
                    .entry(queue_key(&event))
                    .and_modify(|old| {
                        // Reused codes under different catalogue IDs are ambiguous;
                        // keep the track identity without inventing a parent match.
                        if old.as_ref().is_some_and(|r| r.id != race.id) {
                            *old = None;
                        }
                    })
                    .or_insert_with(|| Some(race.clone()));
            }
        }
    }
    state.database.update(|s| {
        s.failures.push(format!("YellowBrick: {} catalogue races; {} URLs resolved; {} races without codes. Details: {}", report.races.len(), report.urls().len(), report.unresolved(), path.display()));
        s.failures.extend(report.warnings.iter().take(20).cloned());
        if report.unresolved() > 0 {
            s.failed += report.unresolved() as u32;
            s.failures.push("Unresolved YellowBrick races were not queued. Check the YellowBrick credentials in Settings or add explicit race URLs.".into());
        }
    });
    Ok(report.urls())
}
fn failure(state: &AppState, url: &str, error: &str) {
    state.database.update(|s| {
        s.failed += 1;
        if s.failures.len() < 100 {
            s.failures.push(format!("{url}: {error}"));
        }
    });
}
pub fn on_startup<R: tauri::Runtime>(app: tauri::AppHandle<R>) {
    if super::settings(&app.state::<AppState>())
        .is_ok_and(|s| s.scrape_schedule == ScrapeSchedule::Startup)
        && let Err(e) = start(app.clone(), "scrape", None)
    {
        app.state::<AppState>()
            .database
            .update(|s| s.error = Some(e.to_string()));
    }
}
/// Quitting waits for all disk/database writers. The shutdown schedule then runs
/// one scrape, after the project save guard, unless that scrape is already running.
pub fn shutdown<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> bool {
    let state = app.state::<AppState>();
    if state.database.shutdown_finished.load(Ordering::SeqCst) {
        return false;
    }
    let scheduled =
        super::settings(&state).is_ok_and(|s| s.scrape_schedule == ScrapeSchedule::Shutdown);
    let (running, scraping) = state
        .database
        .lock()
        .map(|s| (s.progress.running, s.progress.operation == "scrape"))
        .unwrap_or_default();
    if !scheduled && !running {
        return false;
    }
    if state.database.shutdown_started.swap(true, Ordering::SeqCst) {
        return true;
    }
    let revision = state
        .with_session(|s| Ok(s.open.as_ref().map(|o| (o.project.id, o.revision))))
        .ok()
        .flatten();
    if !running && let Err(e) = start(app.clone(), "scrape", None) {
        state.database.update(|s| s.error = Some(e.to_string()));
        state
            .database
            .shutdown_finished
            .store(true, Ordering::SeqCst);
        return false;
    }
    let app = app.clone();
    tauri::async_runtime::spawn_blocking(move || {
        let wait = || {
            while app
                .state::<AppState>()
                .database
                .lock()
                .is_ok_and(|s| s.progress.running)
            {
                std::thread::sleep(Duration::from_millis(100));
            }
        };
        wait();
        if scheduled && running && !scraping {
            if let Err(e) = start(app.clone(), "scrape", None) {
                app.state::<AppState>()
                    .database
                    .update(|s| s.error = Some(e.to_string()));
            }
            wait();
        }
        let state = app.state::<AppState>();
        state
            .database
            .shutdown_finished
            .store(true, Ordering::SeqCst);
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
