#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The environment job end to end (spec.md 7.5, 7.7), against a fake
//! provider and without the network: import queues nothing by itself, a
//! queued fetch fills every sample, a cancelled one keeps what finished and
//! resumes to exactly the uninterrupted result, a failed one is marked, a
//! removed track is left alone, and a derivation change relates the new
//! headings to the stored wind without fetching again.

mod common;

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::env::{self, EnvJobsStatus, JobSink, Outcome};
use pe_app::tracks::{self, TrackFileRequest};
use pe_app::{edit, projects};
use pe_core::canonical::{env_degrees, env_knots};
use pe_core::track::{EnvStatus, Track};
use pe_env::dataset::Dataset;
use pe_env::{EnvError, EnvPoint, Options, Point, Provider, Vector, Waves};

/// 2025-07-26T12:00Z.
const NOON: i64 = 1_753_531_200;
/// A fixed clock, so the recorded fetch times of two runs compare equal.
const NOW: i64 = 1_760_000_000;

/// One boat sailing north up 5°W from 49.5°N, 0.01° every 10 minutes, for
/// ten hours: 61 positions.
fn track_file(root: &TempRoot) -> String {
    let features: Vec<String> = (0..=60)
        .map(|k| {
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[-5.0,{}]}},"properties":{{"time":{},"boat":"Alpha"}}}}"#,
                49.5 + 0.01 * k as f64,
                NOON + 600 * k
            )
        })
        .collect();
    let path = root.file("race.geojson");
    std::fs::write(
        &path,
        format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        ),
    )
    .unwrap();
    path
}

/// A project with the track imported; returns the track's source id.
fn imported(root: &TempRoot) -> (AppState, u64) {
    let app = root.state();
    projects::create(&app, "Env".to_owned(), None, false).unwrap();
    let result = tracks::import(
        &app,
        &[TrackFileRequest {
            path: track_file(root),
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    let id = result.imported[0].source_id;
    (app, id)
}

/// Values that depend only on a position's time and place, as the real
/// archives' do. Wind from WeatherBench2 for the first four hours and
/// ARCO-ERA5 after; current from the NW Shelf north of 49.8°N and
/// GlobCurrent (no tide) south of it.
fn truth(p: &Point) -> EnvPoint {
    let hours = (p.t - NOON) as f64 / 3600.0;
    EnvPoint {
        wind: Some(Vector {
            u: 5.0 + 0.1 * hours,
            v: -3.0 + 0.01 * p.lat,
            dataset: if hours < 4.0 {
                Dataset::Wb2Era5Hourly
            } else {
                Dataset::ArcoEra5
            },
        }),
        waves: Some(Waves {
            hs: Some(1.0 + 0.01 * p.lon),
            from: Some((p.t / 60).rem_euclid(360) as f64),
            dataset: Dataset::ArcoEra5,
        }),
        current: Some(if p.lat > 49.8 {
            Vector {
                u: 0.2,
                v: 0.1,
                dataset: Dataset::CmemsNwsMy,
            }
        } else {
            Vector {
                u: 0.1,
                v: -0.1,
                dataset: Dataset::GlobCurrentMy,
            }
        }),
    }
}

/// The fake archive. `cancel_on` makes the user press Cancel while the
/// call with that number is reading; `fail_on` makes that call's read
/// fail.
struct Fake<'a> {
    calls: AtomicUsize,
    points: AtomicUsize,
    app: Option<&'a AppState>,
    cancel_on: Option<usize>,
    fail_on: Option<usize>,
    /// Said once, as a real provider says a tier would not open.
    warning: Mutex<Option<String>>,
}

impl<'a> Fake<'a> {
    fn new() -> Self {
        Self {
            calls: AtomicUsize::new(0),
            points: AtomicUsize::new(0),
            app: None,
            cancel_on: None,
            fail_on: None,
            warning: Mutex::new(None),
        }
    }
}

impl Provider for Fake<'_> {
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<Vec<EnvPoint>> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst) + 1;
        if Some(call) == self.cancel_on
            && let Some(app) = self.app
        {
            // The user cancels mid-read; the reader sees the flag at its
            // next chunk.
            app.env_jobs.cancel(None);
        }
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        if Some(call) == self.fail_on {
            return Err(EnvError::OutOfRange("the archive answered 500".to_owned()));
        }
        self.points.fetch_add(points.len(), Ordering::SeqCst);
        // Stokes drift adds to the current, as it does in the real store.
        let stokes = if options.stokes_drift { 1.0 } else { 0.0 };
        Ok(points
            .iter()
            .map(|p| {
                let mut env = truth(p);
                if let Some(c) = &mut env.current {
                    c.u += stokes;
                }
                env
            })
            .collect())
    }

    fn take_warnings(&self) -> Vec<String> {
        self.warning.lock().unwrap().take().into_iter().collect()
    }
}

/// Records what the frontend would be sent.
#[derive(Default)]
struct Sink {
    progress: Mutex<Vec<EnvJobsStatus>>,
    changed: AtomicUsize,
}

impl JobSink for Sink {
    fn progress(&self, status: &EnvJobsStatus) {
        self.progress.lock().unwrap().push(status.clone());
    }
    fn changed(&self) {
        self.changed.fetch_add(1, Ordering::SeqCst);
    }
}

/// Runs the queue dry.
fn drain(app: &AppState, provider: &dyn Provider, sink: &Sink) -> Vec<Outcome> {
    drain_at(app, provider, sink, NOW)
}

/// Runs the queue dry with the clock at `now`.
fn drain_at(app: &AppState, provider: &dyn Provider, sink: &Sink, now: i64) -> Vec<Outcome> {
    let mut outcomes = Vec::new();
    while let Some(outcome) = env::run_next(app, provider, sink, now, false) {
        outcomes.push(outcome);
    }
    outcomes
}

fn track(app: &AppState, id: u64) -> Track {
    app.with_session(|session| {
        let open = session.require_open()?;
        Ok(open
            .project
            .source(pe_core::SourceId(id))
            .unwrap()
            .track()
            .unwrap()
            .clone())
    })
    .unwrap()
}

#[test]
fn a_fetch_fills_every_sample_in_project_units() {
    let root = TempRoot::new("env-fill");
    let (app, id) = imported(&root);
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::NotFetched);
    let queued = env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert_eq!(queued.tracks.len(), 1);
    assert_eq!(queued.tracks[0].state, "queued");

    let sink = Sink::default();
    let fake = Fake::new();
    assert_eq!(drain(&app, &fake, &sink), vec![Outcome::Done]);
    let t = track(&app, id);
    assert_eq!(t.env_meta.status, EnvStatus::Ready);
    assert_eq!(t.env_meta.interval_s, Some(3600));
    assert!(t.samples.iter().all(|s| s.env_fetched));
    // Ten hours in 3-hour batches: four calls, every sample once.
    assert_eq!(fake.calls.load(Ordering::SeqCst), 4);
    assert_eq!(fake.points.load(Ordering::SeqCst), 61);
    // Dataset name and version per sample (invariant 3).
    let names: Vec<(&str, Option<bool>)> = t
        .env_meta
        .datasets
        .iter()
        .map(|d| (d.name.as_str(), d.has_tide))
        .collect();
    // In the order samples first referred to them; ARCO-ERA5 wind after
    // 4 h reuses the ARCO record the waves made.
    assert_eq!(
        names,
        [
            ("wb2-era5-1h", None),
            ("arco-era5", None),
            ("globcurrent-my-geo", Some(true)),
            ("cmems-nws-my-uv-geo", Some(true)),
        ]
    );
    assert!(t.env_meta.datasets.iter().all(|d| d.fetched_at == NOW));
    let first = &t.samples[0];
    assert_eq!(first.wind_dataset, Some(0));
    assert_eq!(first.current_dataset, Some(2));
    // u = 5, v = -3 + 0.495 m/s: speed √(u² + v²) × 3600/1852 kn, kept
    // to 0.01 kn (D27).
    let (u, v) = (5.0f64, -3.0 + 0.01 * 49.5);
    assert_eq!(first.tws.unwrap(), env_knots(u.hypot(v) * 3600.0 / 1852.0));
    // The wind blows toward the east-south-east, so it comes from the
    // west-north-west; kept to 0.1°.
    let from = first.twd_from.unwrap();
    assert_eq!(
        from,
        env_degrees(270.0 + (v.abs() / u).atan().to_degrees()),
        "{from}"
    );
    // Related to the motion: the boat heads north, so TWA is the angle to it.
    assert!((first.twa.unwrap() - (360.0 - from)).abs() < 1e-9);
    assert!(first.bsp_corrected.is_some(), "a current was found");
    // Progress reached the frontend, and so did each written batch.
    assert_eq!(sink.changed.load(Ordering::SeqCst), 5);
    let last = sink.progress.lock().unwrap().last().cloned().unwrap();
    assert!(last.tracks.is_empty());
    // With wind, the samples have their place in the polar.
    let summary = projects::summary(&app).unwrap().unwrap();
    let alpha = summary.sources[0].track.clone().unwrap();
    assert_eq!(alpha.env_status, "ready");
    assert_eq!(alpha.env_fetched, 61);
    assert_eq!(alpha.with_wind, 61);
}

/// Runs a fetch on a fresh import, cancelling while the second batch reads
/// when `cancel`; returns the application and the track's id.
fn fetched(root: &TempRoot, cancel: bool) -> (AppState, u64) {
    let (app, id) = imported(root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let outcome = {
        let fake = Fake {
            app: Some(&app),
            cancel_on: cancel.then_some(2),
            ..Fake::new()
        };
        drain(&app, &fake, &Sink::default())
    };
    let expected = if cancel {
        Outcome::Cancelled
    } else {
        Outcome::Done
    };
    assert_eq!(outcome, vec![expected]);
    (app, id)
}

/// Two JSON values equal, numbers to within `tol`.
fn same_json(a: &serde_json::Value, b: &serde_json::Value, tol: f64) -> bool {
    use serde_json::Value;
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => {
            (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() <= tol
        }
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(x, y)| same_json(x, y, tol))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| same_json(v, w, tol)))
        }
        _ => a == b,
    }
}

/// The M9 acceptance: a cancelled job leaves a consistent project that
/// resumes to the same result as an uninterrupted one.
#[test]
fn a_cancelled_fetch_resumes_to_the_uninterrupted_result() {
    let root_a = TempRoot::new("env-straight");
    let (a, id_a) = fetched(&root_a, false);
    let straight = track(&a, id_a);

    // Cancelled while reading its second batch.
    let root_b = TempRoot::new("env-resumed");
    let (b, id_b) = fetched(&root_b, true);
    let partial = track(&b, id_b);
    assert_eq!(partial.env_meta.status, EnvStatus::Partial);
    // The first batch (0–3 h, 18 samples) finished; nothing after it.
    assert_eq!(partial.samples.iter().filter(|s| s.env_fetched).count(), 18);
    assert!(
        partial.samples[18..]
            .iter()
            .all(|s| !s.env_fetched && s.tws.is_none())
    );
    // What finished is exactly what the uninterrupted run found.
    assert_eq!(partial.samples[..18], straight.samples[..18]);

    // Refetch resumes the rest, and only the rest, to exactly the same
    // project data.
    env::queue_fetch(&b, &[id_b], "hourly", false).unwrap();
    let resuming = Fake::new();
    assert_eq!(drain(&b, &resuming, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(
        resuming.points.load(Ordering::SeqCst),
        61 - 18,
        "only the rest"
    );
    let resumed = track(&b, id_b);
    assert_eq!(resumed.samples, straight.samples);
    assert_eq!(resumed.env_meta, straight.env_meta);
}

/// The cancelled project is consistent on disk: saved and reopened it is
/// partial, and a Refetch after reopening completes it. The file holds
/// every value to its canonical precision (knots to 1e-6, degrees to
/// 1e-9), so the result matches the uninterrupted run to that precision.
#[test]
fn a_cancelled_fetch_saves_as_partial_and_resumes_after_reopening() {
    let root_a = TempRoot::new("env-straight-disk");
    let (a, id_a) = fetched(&root_a, false);
    let root_b = TempRoot::new("env-resumed-disk");
    let (b, id_b) = fetched(&root_b, true);
    let path = root_b.file("b.wpsproj");
    projects::save_as(&b, path.clone()).unwrap();
    projects::open(&b, path, false).unwrap();
    let reopened = track(&b, id_b);
    assert_eq!(reopened.env_meta.status, EnvStatus::Partial);
    assert_eq!(
        reopened.samples.iter().filter(|s| s.env_fetched).count(),
        18
    );
    let summary = projects::summary(&b).unwrap().unwrap();
    assert_eq!(
        summary.sources[0].track.as_ref().unwrap().env_status,
        "partial"
    );

    env::queue_fetch(&b, &[id_b], "hourly", false).unwrap();
    assert_eq!(
        drain(&b, &Fake::new(), &Sink::default()),
        vec![Outcome::Done]
    );
    let (ta, tb) = (track(&a, id_a), track(&b, id_b));
    assert_eq!(tb.env_meta, ta.env_meta);
    let json = |t: &Track| serde_json::to_value(&t.samples).unwrap();
    assert!(same_json(&json(&ta), &json(&tb), 1e-5));
}

#[test]
fn a_failed_fetch_keeps_what_finished_and_says_so() {
    let root = TempRoot::new("env-fail");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let failing = Fake {
        fail_on: Some(2),
        ..Fake::new()
    };
    let sink = Sink::default();
    let outcomes = drain(&app, &failing, &sink);
    assert!(matches!(&outcomes[..], [Outcome::Failed(m)] if m.contains("500")));
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::Partial);
    let status = app.env_jobs.status();
    assert_eq!(status.failure.as_ref().unwrap()[0], "Alpha");

    // Failing at once leaves nothing: "failed".
    let root = TempRoot::new("env-fail-first");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let failing = Fake {
        fail_on: Some(1),
        ..Fake::new()
    };
    drain(&app, &failing, &Sink::default());
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::Failed);
}

/// Undoing the import while its fetch waits leaves nothing to write into;
/// the fetch ends without touching the project.
#[test]
fn a_track_removed_under_its_fetch_is_left_alone() {
    let root = TempRoot::new("env-gone");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    edit::undo_last(&app).unwrap();
    let fake = Fake::new();
    assert_eq!(drain(&app, &fake, &Sink::default()), vec![Outcome::Gone]);
    assert_eq!(fake.calls.load(Ordering::SeqCst), 0);
    // Redo brings the track back as it was: not fetched.
    edit::redo_next(&app).unwrap();
    assert_eq!(track(&app, id).env_meta.status, EnvStatus::NotFetched);
}

/// Refetch of a ready track at another interval starts over; a derivation
/// change afterwards relates the new headings to the stored wind without
/// asking the archive again (M8 carry).
#[test]
fn refetch_starts_over_and_rederiving_needs_no_fetch() {
    let root = TempRoot::new("env-refetch");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    drain(&app, &Fake::new(), &Sink::default());
    let coarse = Fake::new();
    env::queue_fetch(&app, &[id], "three_hourly", false).unwrap();
    assert_eq!(drain(&app, &coarse, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(
        coarse.points.load(Ordering::SeqCst),
        61,
        "every sample again"
    );
    assert_eq!(track(&app, id).env_meta.interval_s, Some(10_800));

    let before = track(&app, id);
    let calls = coarse.calls.load(Ordering::SeqCst);
    tracks::track_derivation_set(&app, id, 300, "derived").unwrap();
    let after = track(&app, id);
    assert_eq!(
        coarse.calls.load(Ordering::SeqCst),
        calls,
        "nothing fetched"
    );
    // A 5-minute gap leaves the 10-minute fixes with no derived heading, so
    // no TWA; the stored wind is untouched.
    assert!(
        after
            .samples
            .iter()
            .all(|s| s.heading.is_none() && s.twa.is_none())
    );
    assert!(
        after
            .samples
            .iter()
            .zip(&before.samples)
            .all(|(a, b)| a.tws == b.tws)
    );
    edit::undo_last(&app).unwrap();
    assert_eq!(track(&app, id).samples, before.samples);
}

/// Replacing the project cancels its queued fetches (spec.md 3.3).
#[test]
fn replacing_the_project_cancels_its_jobs() {
    let root = TempRoot::new("env-replace");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    assert!(app.env_jobs.busy());
    projects::create(&app, "Other".to_owned(), None, true).unwrap();
    assert!(!app.env_jobs.busy());
    assert_eq!(drain(&app, &Fake::new(), &Sink::default()), vec![]);
}

#[test]
fn the_estimate_prefers_three_hourly_only_for_a_long_race() {
    let root = TempRoot::new("env-estimate");
    let (app, id) = imported(&root);
    let e = env::estimate_for(&app, &[id], false, None).unwrap();
    assert_eq!(e.samples, 61);
    // Hours 12Z to 22Z: 11 hourly steps, each four 64-byte heads and one
    // block of u, v, wave height and direction (pe-env's estimate test):
    // 1,205,249 bytes, about a tenth of the whole chunks (10.1 MB).
    assert!(e.hourly_bytes >= 11 * 1_205_249, "{}", e.hourly_bytes);
    assert!(
        e.hourly_bytes < 11 * 1_205_249 + 2_000_000,
        "{}",
        e.hourly_bytes
    );
    assert!(e.three_hourly_bytes < e.hourly_bytes);
    assert_eq!(e.recommended, "hourly");
    assert_eq!(e.three_hourly_above_bytes, env::THREE_HOURLY_ABOVE_BYTES);
    // What the project grows by: bytes per sample, not megabytes.
    assert_eq!(e.stored_bytes, 61 * env::STORED_BYTES_PER_SAMPLE);
    assert!(env::queue_fetch(&app, &[id], "weekly", false).is_err());
}

/// Correcting for current and Stokes drift are project settings, each one
/// undo.
#[test]
fn current_correction_and_stokes_drift_are_undoable_settings() {
    let root = TempRoot::new("env-toggles");
    let (app, _) = imported(&root);
    let summary = projects::summary(&app).unwrap().unwrap();
    assert!(summary.use_corrected && !summary.stokes_drift);
    let off = env::use_corrected_set(&app, false).unwrap();
    assert!(!off.use_corrected);
    assert_eq!(off.undo_label.as_deref(), Some("Change current correction"));
    let on = env::stokes_drift_set(&app, true).unwrap();
    assert!(on.stokes_drift);
    edit::undo_last(&app).unwrap();
    let back = edit::undo_last(&app).unwrap();
    assert!(back.use_corrected && !back.stokes_drift);
}

/// Toggling Stokes drift between a cancelled fetch and its Refetch starts
/// over, so no track mixes currents with and without it, and the track
/// records which it has (review round 1).
#[test]
fn a_stokes_change_between_cancel_and_refetch_starts_over() {
    let root = TempRoot::new("env-stokes");
    let (app, id) = fetched(&root, true);
    assert_eq!(track(&app, id).env_meta.stokes_drift, Some(false));
    env::stokes_drift_set(&app, true).unwrap();
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let again = Fake::new();
    assert_eq!(drain(&app, &again, &Sink::default()), vec![Outcome::Done]);
    assert_eq!(
        again.points.load(Ordering::SeqCst),
        61,
        "every sample again"
    );
    let t = track(&app, id);
    assert_eq!(t.env_meta.stokes_drift, Some(true));
    // Every current carries the drift: none is left from the first fetch.
    let expected = |s: &pe_core::track::Sample| {
        let c = truth(&Point {
            t: s.t,
            lat: s.lat,
            lon: s.lon,
        })
        .current
        .unwrap();
        env_knots((c.u + 1.0).hypot(c.v) * 3600.0 / 1852.0)
    };
    assert!(
        t.samples
            .iter()
            .all(|s| s.current_speed.unwrap() == expected(s))
    );
}

/// A restart that is cancelled leaves a track partly fetched at the new
/// settings and otherwise empty: no value, dataset record or fetch time of
/// the earlier fetch survives beside the new ones (review round 1).
#[test]
fn a_cancelled_restart_never_mixes_two_fetches() {
    let root = TempRoot::new("env-restart-cancel");
    let (app, id) = fetched(&root, false);
    const LATER: i64 = NOW + 86_400;
    env::queue_fetch(&app, &[id], "three_hourly", false).unwrap();
    let cancelling = Fake {
        app: Some(&app),
        cancel_on: Some(2),
        ..Fake::new()
    };
    assert_eq!(
        drain_at(&app, &cancelling, &Sink::default(), LATER),
        vec![Outcome::Cancelled]
    );
    let t = track(&app, id);
    assert_eq!(t.env_meta.status, EnvStatus::Partial);
    assert_eq!(t.env_meta.interval_s, Some(10_800));
    let (fetched, rest): (Vec<_>, Vec<_>) = t.samples.iter().partition(|s| s.env_fetched);
    assert!(!fetched.is_empty() && !rest.is_empty());
    for s in &rest {
        assert_eq!(
            (
                s.tws,
                s.twa,
                s.hs_m,
                s.current_speed,
                s.bsp_corrected,
                s.wind_dataset,
                s.current_dataset
            ),
            (None, None, None, None, None, None, None)
        );
    }
    // Only the new fetch's records, each one used.
    assert!(t.env_meta.datasets.iter().all(|d| d.fetched_at == LATER));
    let used: std::collections::BTreeSet<u16> = fetched
        .iter()
        .flat_map(|s| [s.wind_dataset, s.wave_dataset, s.current_dataset])
        .flatten()
        .collect();
    assert_eq!(used.len(), t.env_meta.datasets.len());
}

/// A provider's warning (a current source left out) reaches the job's
/// status with the track's name, and the fetch still completes.
#[test]
fn a_provider_warning_reaches_the_status() {
    let root = TempRoot::new("env-warning");
    let (app, id) = imported(&root);
    env::queue_fetch(&app, &[id], "hourly", false).unwrap();
    let warning = Fake::new();
    *warning.warning.lock().unwrap() = Some("the current source x could not be opened".to_owned());
    assert_eq!(drain(&app, &warning, &Sink::default()), vec![Outcome::Done]);
    let status = app.env_jobs.status();
    assert_eq!(
        status.warning,
        Some(vec![
            "Alpha".to_owned(),
            "the current source x could not be opened".to_owned()
        ])
    );
    assert!(status.failure.is_none());
}
