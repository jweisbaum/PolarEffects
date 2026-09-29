#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Tracker imports end to end (spec.md 7.2, 7.6), from recorded YellowBrick
//! responses served locally: download the whole event, keep it for the
//! session, import chosen boats as one undo entry with their start and
//! finish as the time window, and save and reopen with the origin intact.

mod common;

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::{edit, projects, trackers};
use pe_core::track::{TrackOrigin, Tracker};
use pe_trackers::yellowbrick::YellowBrick;

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../pe-trackers/tests/fixtures/yellowbrick")
            .join(path),
    )
    .expect(path)
}

/// Serves the Middle Sea Race 2024 recordings and counts the requests.
fn serve() -> (String, Arc<AtomicUsize>) {
    let routes: Vec<(&str, Vec<u8>)> = vec![
        (
            "/JSON/rmsr2024/RaceSetup",
            fixture("rmsr2024-RaceSetup.json"),
        ),
        (
            "/BIN/rmsr2024/AllPositions3",
            fixture("rmsr2024-AllPositions3-first3.bin"),
        ),
    ];
    let requests = Arc::new(AtomicUsize::new(0));
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    let counter = Arc::clone(&requests);
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            counter.fetch_add(1, Ordering::SeqCst);
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
            let (status, body) = routes
                .iter()
                .find(|(p, _)| *p == path)
                .map_or(("404 Not Found", Vec::new()), |(_, b)| {
                    ("200 OK", b.clone())
                });
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    (format!("http://127.0.0.1:{port}"), requests)
}

fn project(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Middle Sea".to_owned(), None, false).unwrap();
    app
}

fn download(app: &AppState, host: &str) -> trackers::TrackerEventView {
    let mut fractions = Vec::new();
    let view = trackers::download_with(
        app,
        Arc::new(YellowBrick::at(host, host)),
        "yb.tl/rmsr2024",
        false,
        |p| fractions.push(p.fraction),
    )
    .unwrap();
    if !view.cached {
        assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{fractions:?}");
    }
    view
}

#[test]
fn an_event_downloads_once_per_session_and_imports_chosen_boats() {
    let root = TempRoot::new("trackers-import");
    let app = project(&root);
    let (host, requests) = serve();

    let view = download(&app, &host);
    assert!(!view.cached);
    assert_eq!(view.tracker, "yellowbrick");
    assert_eq!(view.key, "rmsr2024");
    assert_eq!(view.title, "Rolex Middle Sea Race 2024");
    assert_eq!(view.boats.len(), 112);
    let first = &view.boats[0];
    assert_eq!(
        (first.name.as_str(), first.sail.as_deref(), first.fixes),
        ("12 NACIRA 69", Some("ITA17498"), 1689)
    );
    assert_eq!(first.preview.len(), 2 * trackers::PREVIEW_POINTS);
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    // The second time comes from the session: no request.
    let again = download(&app, &host);
    assert!(again.cached);
    assert_eq!(again.boats.len(), 112);
    assert_eq!(requests.load(Ordering::SeqCst), 2);

    // Boats 1 and 2 import; boat 4 has no positions in the cropped binary.
    let result = trackers::import_boats(
        &app,
        Tracker::YellowBrick,
        "rmsr2024",
        &["1".to_owned(), "2".to_owned(), "4".to_owned()],
    )
    .unwrap();
    assert_eq!(result.imported.len(), 2);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].reason, "no-positions");
    let labels: Vec<&str> = result.imported.iter().map(|l| l.label.as_str()).collect();
    assert_eq!(labels, ["12 NACIRA 69", "AFAZIK IMPULSE"]);
    // YellowBrick gives no course or speed: both derived.
    assert_eq!(result.imported[0].heading_given, 0);
    assert!(result.imported[0].heading_derived > 1600);
    let sources = &result.project.sources;
    assert_eq!(sources.len(), 2);
    assert_ne!(sources[0].colour, sources[1].colour);
    let track = sources[0].track.as_ref().unwrap();
    assert_eq!(track.origin, "tracker");
    assert_eq!(track.event_title, "Rolex Middle Sea Race 2024");
    // The boat's own start and finish (RaceSetup teams[0]).
    assert_eq!(track.filters.time_start, Some(1_729_332_000));
    assert_eq!(track.filters.time_end, Some(1_729_722_454));
    assert!(track.filtered > 0, "pre-start positions are filtered out");

    app.with_session(|session| {
        let open = session.require_open()?;
        let source = &open.project.sources[0];
        let TrackOrigin::Tracker {
            tracker,
            event_url,
            boat_id,
            sail_no,
            model,
            division,
            race_start,
            race_finish,
            ..
        } = &source.track().unwrap().origin
        else {
            panic!("a tracker origin");
        };
        assert_eq!(*tracker, Tracker::YellowBrick);
        assert_eq!(
            *event_url,
            format!("{}/rmsr2024", pe_trackers::yellowbrick::SITE)
        );
        assert_eq!(boat_id, "1");
        assert_eq!(sail_no.as_deref(), Some("ITA17498"));
        assert_eq!(model.as_deref(), Some("NACIRA V69 4.25"));
        assert_eq!(
            division.as_deref(),
            Some("Line Honours Monohull, IRC Overall, IRC Class 2")
        );
        assert_eq!(
            (*race_start, *race_finish),
            (Some(1_729_332_000), Some(1_729_722_454))
        );
        Ok(())
    })
    .unwrap();

    // One undo takes the whole import back out.
    let after = edit::undo_last(&app).unwrap();
    assert!(after.sources.is_empty());
    edit::redo_next(&app).unwrap();

    // The origin survives a save and reopen, byte for byte.
    let path = root.file("rmsr.wpsproj");
    projects::save_as(&app, path.clone()).unwrap();
    let saved = std::fs::read(&path).unwrap();
    projects::open(&app, path.clone(), true).unwrap();
    projects::save(&app).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), saved);
}

#[test]
fn importing_needs_the_event_downloaded_this_session() {
    let root = TempRoot::new("trackers-missing");
    let app = project(&root);
    let err = trackers::import_boats(&app, Tracker::YellowBrick, "rmsr2024", &["1".to_owned()])
        .expect_err("not downloaded");
    assert_eq!(err.kind(), "doing");
}

#[test]
fn an_address_of_another_host_is_refused_before_any_request() {
    let root = TempRoot::new("trackers-address");
    let app = project(&root);
    let err = trackers::download_with(
        &app,
        Arc::new(YellowBrick::default()),
        "https://example.invalid/fastnet2025",
        false,
        |_| {},
    )
    .expect_err("refused");
    assert_eq!(err.kind(), "tracker-address");
}

/// Cancel returns at once, even while the tracker has not answered at all
/// (spec.md 7.7), and nothing is kept.
#[test]
fn cancel_ends_a_download_that_is_waiting_on_the_network() {
    let root = TempRoot::new("trackers-cancel");
    let app = Arc::new(project(&root));
    // Accepts and never answers.
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let host = format!("http://127.0.0.1:{}", listener.local_addr().unwrap().port());
    let _held = std::thread::spawn(move || {
        let held: Vec<_> = listener.incoming().take(1).collect();
        std::thread::sleep(std::time::Duration::from_secs(20));
        drop(held);
    });
    let canceller = {
        let app = Arc::clone(&app);
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(300));
            app.trackers.cancel();
        })
    };
    let start = std::time::Instant::now();
    let err = trackers::download_with(
        &app,
        Arc::new(YellowBrick::at(&host, &host)),
        "rmsr2024",
        false,
        |_| {},
    )
    .expect_err("cancelled");
    assert_eq!(err.kind(), "cancelled");
    assert!(
        start.elapsed() < std::time::Duration::from_secs(5),
        "{:?}",
        start.elapsed()
    );
    canceller.join().unwrap();
    assert!(
        trackers::import_boats(&app, Tracker::YellowBrick, "rmsr2024", &["1".to_owned()]).is_err(),
        "a cancelled download is not kept"
    );
}

/// Serves the 24 Heures Ultim 2025 recordings (Geovoile); a route ending in
/// `*` matches every path it starts.
fn serve_geovoile() -> String {
    let file = |name: &str| {
        std::fs::read(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../pe-trackers/tests/fixtures/geovoile/24hultim2025")
                .join(name),
        )
        .expect(name)
    };
    let routes: Vec<(&str, Vec<u8>)> = vec![
        ("/2025/tracker/", file("viewer.html")),
        ("/2025/tracker/resources/versions/v*", file("versions.txt")),
        (
            "/2025/tracker/resources/config/v20251006074618",
            file("config.hwx"),
        ),
        (
            "/2025/tracker/resources/tracks/v20250928152939",
            file("tracks.hwx"),
        ),
        (
            "/2025/tracker/resources/reports/v20250928152939",
            file("reports.hwx"),
        ),
    ];
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
            let (status, body) = routes
                .iter()
                .find(|(p, _)| {
                    p.strip_suffix('*')
                        .map_or(*p == path, |prefix| path.starts_with(prefix))
                })
                .map_or(("404 Not Found", Vec::new()), |(_, b)| {
                    ("200 OK", b.clone())
                });
            let head = format!(
                "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    format!("http://127.0.0.1:{port}")
}

/// A Geovoile event through the shared dialog flow (M11): the reports'
/// official heading and speed are used as given, the boat's arrival ends
/// its time window, and the origin records the canonical viewer address.
#[test]
fn a_geovoile_event_imports_with_official_heading_and_speed() {
    let root = TempRoot::new("trackers-geovoile");
    let app = project(&root);
    let host = serve_geovoile();
    let mut fractions = Vec::new();
    let view = trackers::download_with(
        &app,
        Arc::new(pe_trackers::geovoile::Geovoile::at(&host)),
        "24hultim.geovoile.com/2025/tracker/",
        false,
        |p| fractions.push(p.fraction),
    )
    .unwrap();
    assert!(fractions.windows(2).all(|w| w[0] <= w[1]), "{fractions:?}");
    assert_eq!(view.tracker, "geovoile");
    assert_eq!(view.key, "24hultim.geovoile.com/2025/");
    assert_eq!(view.title, "24H Ultim");
    assert_eq!((view.leg, view.legs), (None, None));
    assert_eq!(view.boats.len(), 14);
    let result =
        trackers::import_boats(&app, Tracker::Geovoile, &view.key, &["4".to_owned()]).unwrap();
    assert_eq!(result.imported.len(), 1);
    // 246 of its 470 fixes carry the reports' heading (see pe-trackers'
    // fixture test); the rest are derived.
    assert_eq!(result.imported[0].heading_given, 246);
    let track = result.project.sources[0].track.as_ref().unwrap();
    assert_eq!(track.event_title, "24H Ultim");
    // The start of its run and its official arrival.
    assert_eq!(track.filters.time_start, Some(1_758_967_200));
    assert_eq!(track.filters.time_end, Some(1_759_041_172));
    app.with_session(|session| {
        let open = session.require_open()?;
        let TrackOrigin::Tracker {
            tracker,
            event_url,
            division,
            ..
        } = &open.project.sources[0].track().unwrap().origin
        else {
            panic!("a tracker origin");
        };
        assert_eq!(*tracker, Tracker::Geovoile);
        assert_eq!(
            *event_url,
            pe_trackers::geovoile::site("24hultim.geovoile.com/2025/tracker/")
                .unwrap()
                .url()
        );
        assert!(division.is_some());
        Ok(())
    })
    .unwrap();
}

/// A reanalysis archive that fails on any request and counts them.
#[derive(Default)]
struct Refusing(AtomicUsize);

impl pe_env::Provider for Refusing {
    fn sample(
        &self,
        _points: &[pe_env::Point],
        _options: &pe_env::Options,
        _cancel: &Arc<std::sync::atomic::AtomicBool>,
    ) -> pe_env::Result<Vec<pe_env::EnvPoint>> {
        self.0.fetch_add(1, Ordering::SeqCst);
        Err(pe_env::EnvError::Open(
            "no reanalysis request may be made".to_owned(),
        ))
    }
}

#[derive(Default)]
struct Quiet;

impl pe_app::env::JobSink for Quiet {
    fn progress(&self, _status: &pe_app::env::EnvJobsStatus) {}
    fn changed(&self) {}
}

/// Importing is tracks only (D24): neither a tracker import nor a file
/// import queues a weather fetch, opens the reanalysis archives, or makes
/// a single reanalysis request, even when the job runner is driven.
#[test]
fn importing_issues_no_reanalysis_request() {
    let root = TempRoot::new("trackers-no-weather");
    let app = project(&root);
    let (host, _) = serve();
    download(&app, &host);
    let from_tracker =
        trackers::import_boats(&app, Tracker::YellowBrick, "rmsr2024", &["1".to_owned()]).unwrap();
    assert_eq!(from_tracker.imported.len(), 1);
    let path = root.file("one.geojson");
    std::fs::write(
        &path,
        r#"{"type":"FeatureCollection","features":[
{"type":"Feature","geometry":{"type":"Point","coordinates":[14.5,35.9]},"properties":{"time":1729233205}},
{"type":"Feature","geometry":{"type":"Point","coordinates":[14.6,35.95]},"properties":{"time":1729236805}}]}"#,
    )
    .unwrap();
    let from_file = pe_app::tracks::import(
        &app,
        &[pe_app::tracks::TrackFileRequest {
            path,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    assert_eq!(from_file.imported.len(), 1);

    assert!(app.env_jobs.status().tracks.is_empty(), "nothing queued");
    let refusing = Refusing::default();
    assert!(pe_app::env::run_next(&app, &refusing, &Quiet, 1_760_000_000, false).is_none());
    assert_eq!(
        refusing.0.load(Ordering::SeqCst),
        0,
        "no reanalysis request"
    );
    assert!(
        app.env_provider.lock().unwrap().is_none(),
        "the reanalysis archives were never opened"
    );
    for source in &from_file.project.sources {
        if let Some(track) = &source.track {
            assert_eq!(track.env_status, "not_fetched");
        }
    }
}
