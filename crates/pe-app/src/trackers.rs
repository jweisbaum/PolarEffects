//! Tracker imports (spec.md 7.2): download a whole event, keep it for the
//! session, and import the boats the user picks.
//!
//! Two calls, as the dialog needs. [`tracker_event`] resolves the pasted
//! address and downloads **every** boat's full track as a job with
//! progress (`tracker://progress`) and Cancel ([`cancel_tracker_event`]);
//! the event is kept in memory, so asking again, or importing a second boat
//! later in the session, downloads nothing. [`import_tracker_boats`] then
//! builds one track source per chosen boat as one undoable change, exactly
//! as a file import does, with the boat's start and finish as the default
//! time window (spec.md 7.6).

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use pe_core::source::{SampleFilters, TimeWindow};
use pe_core::track::{DerivationSettings, TrackOrigin, Tracker};
use pe_core::{SampleId, TrackId};
use pe_trackers::event::PositionsFrom;
use pe_trackers::{Fetcher, Progress, TrackerClient, TrackerError, TrackerEvent};
use pe_tracks::build_track;
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::tracks::{Pending, TrackImportFailure, TrackImportResult, add_tracks, count};

/// The event carrying [`TrackerProgress`] while an event downloads.
pub const PROGRESS_EVENT: &str = "tracker://progress";

/// How many downloaded events the session keeps; the oldest goes first.
/// The Fastnet 2025 (444 boats, 714,380 positions) is about 30 MB in memory.
pub const KEPT_EVENTS: usize = 4;

/// How many points of each boat the dialog's map preview gets.
pub const PREVIEW_POINTS: usize = 64;

/// How often progress is reported at most.
const PROGRESS_EVERY: Duration = Duration::from_millis(100);

/// The events downloaded this session, and the download running now.
#[derive(Debug, Default)]
pub struct TrackerSession {
    events: Mutex<Vec<Arc<TrackerEvent>>>,
    cancel: Mutex<Option<Arc<AtomicBool>>>,
}

impl TrackerSession {
    fn cached(&self, tracker: Tracker, key: &str) -> Option<Arc<TrackerEvent>> {
        self.events
            .lock()
            .ok()?
            .iter()
            .find(|e| e.event.tracker == tracker && e.event.key == key)
            .cloned()
    }

    fn keep(&self, event: Arc<TrackerEvent>) {
        if let Ok(mut events) = self.events.lock() {
            events.retain(|e| {
                !(e.event.tracker == event.event.tracker && e.event.key == event.event.key)
            });
            events.push(event);
            let excess = events.len().saturating_sub(KEPT_EVENTS);
            events.drain(..excess);
        }
    }

    /// Stops the running download, if any.
    pub fn cancel(&self) {
        if let Ok(slot) = self.cancel.lock()
            && let Some(flag) = slot.as_ref()
        {
            flag.store(true, Ordering::SeqCst);
        }
    }
}

// ----------------------------------------------------------------- wire

/// A tracker's wire name.
fn tracker_of(name: &str) -> Result<Tracker> {
    match name {
        "yellowbrick" => Ok(Tracker::YellowBrick),
        "geovoile" => Ok(Tracker::Geovoile),
        "bluewater" => Ok(Tracker::BlueWaterTracks),
        other => Err(AppError::BadOption {
            field: "Tracker",
            value: other.to_owned(),
        }),
    }
}

fn tracker_name(tracker: Tracker) -> &'static str {
    match tracker {
        Tracker::YellowBrick => "yellowbrick",
        Tracker::Geovoile => "geovoile",
        Tracker::BlueWaterTracks => "bluewater",
    }
}

fn client_of(tracker: Tracker) -> Result<Box<dyn TrackerClient>> {
    pe_trackers::event::client(tracker).ok_or_else(|| AppError::BadOption {
        field: "Tracker",
        value: pe_trackers::event::name(tracker).to_owned(),
    })
}

/// How far a download is (spec.md 7.7).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerProgress.ts")]
pub struct TrackerProgress {
    /// 0–1.
    pub fraction: f64,
    /// Bytes of the response being read.
    pub bytes: f64,
    /// Whether the fallback format is being read (YellowBrick's KML).
    pub fallback: bool,
}

/// One boat of a downloaded event, as the dialog's table shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerBoatRow.ts")]
pub struct TrackerBoatRow {
    /// The tracker's id for the boat.
    pub id: String,
    /// Boat name.
    pub name: String,
    /// Sail number.
    pub sail: Option<String>,
    /// Model or class.
    pub model: Option<String>,
    /// Division.
    pub division: Option<String>,
    /// The tracker's status (`RACING`, `FINISHED`, `RETIRED`…).
    pub status: Option<String>,
    /// Positions.
    pub fixes: u32,
    /// First position, UTC epoch seconds.
    pub first: Option<i64>,
    /// Last position.
    pub last: Option<i64>,
    /// A few positions for the map preview: `[lon, lat, lon, lat, …]`.
    pub preview: Vec<f64>,
}

/// A downloaded event, as the dialog shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "TrackerEventView.ts")]
pub struct TrackerEventView {
    /// `"yellowbrick"`, `"geovoile"` or `"bluewater"`.
    pub tracker: String,
    /// The tracker's key for the event.
    pub key: String,
    /// The canonical address.
    pub url: String,
    /// Title.
    pub title: String,
    /// Event start, UTC epoch seconds.
    pub start: Option<i64>,
    /// Event end.
    pub stop: Option<i64>,
    /// Whether the positions came from the fallback format.
    pub fallback: bool,
    /// Whether this came from the session's memory rather than a download.
    pub cached: bool,
    /// Every boat.
    pub boats: Vec<TrackerBoatRow>,
}

/// Up to [`PREVIEW_POINTS`] evenly spaced positions, the last included.
fn preview(fixes: &[pe_core::track::Fix]) -> Vec<f64> {
    let n = fixes.len();
    if n == 0 {
        return Vec::new();
    }
    let take = n.min(PREVIEW_POINTS);
    (0..take)
        .map(|k| {
            if take == 1 {
                0
            } else {
                k * (n - 1) / (take - 1)
            }
        })
        .flat_map(|i| [fixes[i].lon, fixes[i].lat])
        .collect()
}

impl TrackerEventView {
    fn of(event: &TrackerEvent, cached: bool) -> Self {
        Self {
            tracker: tracker_name(event.event.tracker).to_owned(),
            key: event.event.key.clone(),
            url: event.event.url.clone(),
            title: event.title.clone(),
            start: event.start,
            stop: event.stop,
            fallback: event.positions_from == PositionsFrom::Fallback,
            cached,
            boats: event
                .boats
                .iter()
                .map(|b| TrackerBoatRow {
                    id: b.id.clone(),
                    name: b.name.clone(),
                    sail: b.sail.clone(),
                    model: b.model.clone(),
                    division: b.division.clone(),
                    status: b.status.clone(),
                    fixes: count(b.fixes.len()),
                    first: b.fixes.first().map(|f| f.t),
                    last: b.fixes.last().map(|f| f.t),
                    preview: preview(&b.fixes),
                })
                .collect(),
        }
    }
}

// ------------------------------------------------------------- download

/// Downloads (or recalls) the event `input` names with `client`, reporting
/// progress to `on_progress`. `refresh` downloads again even when the
/// session has it. Returns at once on Cancel; a download that is still
/// waiting on the network then finishes on its own and is dropped.
///
/// # Errors
/// [`AppError::Tracker`] for an address the tracker does not serve, a
/// failed or cancelled download, or a response that did not decode.
pub fn download_with(
    state: &AppState,
    client: Arc<dyn TrackerClient>,
    input: &str,
    refresh: bool,
    mut on_progress: impl FnMut(TrackerProgress),
) -> Result<TrackerEventView> {
    let event = client.resolve(input)?;
    if !refresh && let Some(cached) = state.trackers.cached(event.tracker, &event.key) {
        return Ok(TrackerEventView::of(&cached, true));
    }
    let timeout = state.with_session(|s| Ok(s.settings.network.timeout_s))?;
    let cancel = Arc::new(AtomicBool::new(false));
    if let Ok(mut slot) = state.trackers.cancel.lock() {
        // A new download replaces (and stops) any earlier one.
        if let Some(old) = slot.replace(Arc::clone(&cancel)) {
            old.store(true, Ordering::SeqCst);
        }
    }
    let fetcher = Fetcher::new(
        pe_trackers::event::name(event.tracker),
        Duration::from_secs(u64::from(timeout.max(1))),
        Arc::clone(&cancel),
    )?;
    let (tx, rx) = std::sync::mpsc::channel::<
        std::result::Result<Progress, pe_trackers::Result<TrackerEvent>>,
    >();
    let worker = {
        let tx = tx.clone();
        let event = event.clone();
        std::thread::Builder::new()
            .name("tracker-download".to_owned())
            .spawn(move || {
                let result = client.fetch(&event, &fetcher, &mut |p| {
                    let _ = tx.send(Ok(p));
                });
                let _ = tx.send(Err(result));
            })
            .map_err(|e| AppError::Internal(format!("no download thread: {e}")))?
    };
    drop(tx);
    let mut last = Instant::now() - PROGRESS_EVERY;
    let mut last_step = u32::MAX;
    let outcome = loop {
        if cancel.load(Ordering::SeqCst) {
            break Err(TrackerError::Cancelled);
        }
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(Ok(p)) => {
                if p.step != last_step || last.elapsed() >= PROGRESS_EVERY {
                    last = Instant::now();
                    last_step = p.step;
                    on_progress(TrackerProgress {
                        fraction: p.fraction(),
                        bytes: p.bytes as f64,
                        fallback: p.steps > 2 && p.step >= 2,
                    });
                }
            }
            Ok(Err(result)) => break result,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                break Err(TrackerError::Network("the download stopped".to_owned()));
            }
        }
    };
    if let Ok(mut slot) = state.trackers.cancel.lock()
        && slot.as_ref().is_some_and(|f| Arc::ptr_eq(f, &cancel))
    {
        *slot = None;
    }
    if outcome.is_ok() {
        let _ = worker.join();
    }
    let downloaded = Arc::new(outcome?);
    state.trackers.keep(Arc::clone(&downloaded));
    Ok(TrackerEventView::of(&downloaded, false))
}

/// Resolves a pasted event address and downloads every boat's track, or
/// recalls the event from this session (spec.md 7.2).
#[tauri::command]
pub async fn tracker_event(
    app: tauri::AppHandle,
    tracker: String,
    url: String,
    refresh: bool,
) -> Result<TrackerEventView> {
    use tauri::{Emitter, Manager};
    let tracker = tracker_of(&tracker)?;
    let client: Arc<dyn TrackerClient> = Arc::from(client_of(tracker)?);
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        download_with(&state, client, &url, refresh, |p| {
            let _ = app.emit(PROGRESS_EVENT, &p);
        })
    })
    .await
    .map_err(|e| AppError::Internal(format!("the download task failed: {e}")))?
}

/// Stops the running event download.
#[tauri::command]
pub fn cancel_tracker_event(state: tauri::State<'_, AppState>) -> Result<()> {
    state.trackers.cancel();
    Ok(())
}

// --------------------------------------------------------------- import

/// The track of one boat, ready for its ids.
fn pending(event: &TrackerEvent, boat: &pe_trackers::TrackerBoat) -> Pending {
    let label = if boat.name.is_empty() {
        boat.sail.clone().unwrap_or_else(|| boat.id.clone())
    } else {
        boat.name.clone()
    };
    let origin = TrackOrigin::Tracker {
        tracker: event.event.tracker,
        event_url: event.event.url.clone(),
        event_title: event.title.clone(),
        boat_id: boat.id.clone(),
        boat_name: label.clone(),
        sail_no: boat.sail.clone(),
        model: boat.model.clone(),
        division: boat.division.clone(),
        race_start: boat.start,
        race_finish: boat.finish,
    };
    let mut next = 0u64;
    let (track, report) = build_track(
        TrackId(0),
        origin,
        boat.fixes.clone(),
        DerivationSettings::default(),
        || {
            next += 1;
            SampleId(next)
        },
    );
    // The default time window is the boat's start and finish, so pre-start
    // and post-finish motoring is out (spec.md 7.6).
    let mut filters = SampleFilters::default();
    if boat.start.is_some() || boat.finish.is_some() {
        filters.time_window = Some(TimeWindow {
            start: boat.start,
            end: boat
                .finish
                .filter(|end| boat.start.is_none_or(|start| *end > start)),
        });
    }
    Pending {
        file: event.title.clone(),
        label,
        track,
        report,
        filters,
    }
}

/// [`import_tracker_boats`] without a Tauri handle.
pub fn import_boats(
    state: &AppState,
    tracker: Tracker,
    key: &str,
    boats: &[String],
) -> Result<TrackImportResult> {
    state.with_session(|session| session.require_open().map(|_| ()))?;
    let event = state
        .trackers
        .cached(tracker, key)
        .ok_or_else(|| AppError::Doing {
            doing: "import from",
            what: key.to_owned(),
            why: "the event is no longer downloaded in this session; open it again".to_owned(),
        })?;
    let mut pendings = Vec::new();
    let mut failures = Vec::new();
    for id in boats {
        let Some(boat) = event.boat(id) else {
            failures.push(TrackImportFailure::simple(
                id,
                "no-boat",
                format!("{}: no boat {id}", event.title),
            ));
            continue;
        };
        if boat.fixes.is_empty() {
            failures.push(TrackImportFailure::simple(
                &boat.name,
                "no-positions",
                format!(
                    "{}: the tracker has no positions for {}",
                    event.title, boat.name
                ),
            ));
            continue;
        }
        pendings.push(pending(&event, boat));
    }
    add_tracks(state, pendings, failures)
}

/// Imports the chosen boats of a downloaded event, one track source each,
/// as one undoable change (spec.md 7.2).
#[tauri::command]
pub async fn import_tracker_boats(
    state: tauri::State<'_, AppState>,
    tracker: String,
    key: String,
    boats: Vec<String>,
) -> Result<TrackImportResult> {
    import_boats(&state, tracker_of(&tracker)?, &key, &boats)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_keep_the_ends_and_stay_small() {
        let fix = |t: i64| pe_core::track::Fix {
            t,
            lat: t as f64,
            lon: -(t as f64),
            cog: None,
            sog: None,
        };
        let fixes: Vec<_> = (0..1000).map(fix).collect();
        let p = preview(&fixes);
        assert_eq!(p.len(), 2 * PREVIEW_POINTS);
        assert_eq!(&p[..2], &[0.0, 0.0]);
        assert_eq!(&p[p.len() - 2..], &[-999.0, 999.0]);
        assert_eq!(preview(&fixes[..1]), vec![0.0, 0.0]);
        assert!(preview(&[]).is_empty());
    }

    #[test]
    fn tracker_names_cross_the_wire() {
        for t in [
            Tracker::YellowBrick,
            Tracker::Geovoile,
            Tracker::BlueWaterTracks,
        ] {
            assert_eq!(tracker_of(tracker_name(t)).expect("known"), t);
        }
        assert!(tracker_of("nope").is_err());
    }
}
