#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! How fast a tracker event reaches the dialog (plan.md M14b): live, never
//! in the default suite (`#[ignore]`, `PE_TEST_LIVE=1`).
//!
//! ```text
//! PE_TEST_LIVE=1 cargo test -p pe-app --test tracker_speed -- --ignored --nocapture --test-threads=1
//! ```
//!
//! For each event it prints the time from the call to the first boat list
//! (the listing), to the full event (positions and previews), the time to
//! recall it from the session, the IPC payload of each (serialised as Tauri
//! does, with serde_json) and the time to import one and ten boats.

mod common;

use std::sync::Arc;
use std::time::Instant;

use common::TempRoot;
use pe_app::{projects, trackers};
use pe_trackers::TrackerClient;

fn live() -> bool {
    std::env::var("PE_TEST_LIVE").is_ok_and(|v| v == "1")
}

fn measure(label: &str, client: Arc<dyn TrackerClient>, input: &str) {
    let root = TempRoot::new("tracker-speed");
    let app = root.state();
    projects::create(&app, "Speed".to_owned(), None, false).unwrap();
    let tracker = client.tracker();

    let start = Instant::now();
    let mut listed: Option<(f64, usize, usize)> = None;
    let mut steps: Vec<(f64, f64)> = Vec::new();
    let view = trackers::download_listed(
        &app,
        client,
        input,
        true,
        |p| {
            if steps
                .last()
                .is_none_or(|(_, f)| (p.fraction - f).abs() > 0.2)
            {
                steps.push((start.elapsed().as_secs_f64(), p.fraction));
            }
        },
        |list| {
            let bytes = serde_json::to_vec(&list).unwrap().len();
            listed = Some((start.elapsed().as_secs_f64(), list.boats.len(), bytes));
        },
    )
    .unwrap();
    let full = start.elapsed().as_secs_f64();
    let t = Instant::now();
    let payload = serde_json::to_vec(&view).unwrap();
    let serialise = t.elapsed().as_secs_f64();
    let t = Instant::now();
    let again = trackers::download_listed(
        &app,
        Arc::from(pe_trackers::event::client(tracker).unwrap()),
        input,
        false,
        |_| {},
        |_| {},
    )
    .unwrap();
    let recall = t.elapsed().as_secs_f64();
    assert!(again.cached);
    let fixes: u64 = view.boats.iter().map(|b| u64::from(b.fixes)).sum();
    let ids: Vec<String> = view
        .boats
        .iter()
        .filter(|b| b.fixes > 0)
        .map(|b| b.id.clone())
        .collect();
    let t = Instant::now();
    trackers::import_boats(&app, tracker, &view.key, &ids[..1]).unwrap();
    let one = t.elapsed().as_secs_f64();
    let t = Instant::now();
    trackers::import_boats(&app, tracker, &view.key, &ids[..ids.len().min(10)]).unwrap();
    let ten = t.elapsed().as_secs_f64();
    let listed = listed.map_or("none".to_owned(), |(at, n, b)| {
        format!("{at:.2} s ({n} boats, {:.0} KB)", b as f64 / 1024.0)
    });
    println!(
        "M14b | {label}: {} boats, {fixes} fixes | boat list {listed} | full event {full:.2} s ({:.0} KB, serialised in {:.0} ms) | recall {:.0} ms | import 1 boat {:.0} ms, {} boats {:.0} ms | progress {steps:?}",
        view.boats.len(),
        payload.len() as f64 / 1024.0,
        serialise * 1e3,
        recall * 1e3,
        one * 1e3,
        ids.len().min(10),
        ten * 1e3,
    );
}

#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn yellowbrick_fastnet_2025() {
    if !live() {
        return;
    }
    measure(
        "YellowBrick fastnet2025",
        Arc::new(pe_trackers::yellowbrick::YellowBrick::default()),
        "yb.tl/fastnet2025",
    );
}

#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn geovoile_new_york_vendee_2024() {
    if !live() {
        return;
    }
    measure(
        "Geovoile newyorkvendee 2024",
        Arc::new(pe_trackers::geovoile::Geovoile::default()),
        "newyorkvendee.geovoile.com/2024/tracker/",
    );
}

#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn geovoile_figaro_2024_leg_1() {
    if !live() {
        return;
    }
    measure(
        "Geovoile lasolitaire 2024 leg 1",
        Arc::new(pe_trackers::geovoile::Geovoile::default()),
        "lasolitaire.geovoile.com/2024/tracker/?leg=1",
    );
}

#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn bluewater_melbourne_hobart_2025() {
    if !live() {
        return;
    }
    measure(
        "Blue Water melbourne-hobart-westcoaster 2025",
        Arc::new(pe_trackers::bluewater::BlueWaterTracks::default()),
        "2025-melbourne-hobart-westcoaster",
    );
}

/// The KML fallback as a race whose binary does not load meets it: the
/// Middle Sea Race 2024's own RaceSetup (fetched live, served locally) and
/// an `AllPositions3` that answers a web page, so the client reads the real
/// `yb.tl` KML (built on request, 23 MB).
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn yellowbrick_kml_fallback_rmsr_2024() {
    use std::io::{Read, Write};
    if !live() {
        return;
    }
    let fetcher = pe_trackers::Fetcher::new(
        "YellowBrick",
        std::time::Duration::from_secs(60),
        Arc::default(),
    )
    .unwrap();
    let setup = fetcher
        .get(
            &format!("{}/JSON/rmsr2024/RaceSetup", pe_trackers::yellowbrick::CDN),
            &mut |_, _| {},
        )
        .unwrap();
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let body: Vec<u8> = if request.contains("RaceSetup") {
                setup.clone()
            } else {
                b"<!DOCTYPE html><html></html>".to_vec()
            };
            let head = format!(
                "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            );
            let _ = stream.write_all(head.as_bytes());
            let _ = stream.write_all(&body);
        }
    });
    measure(
        "YellowBrick rmsr2024 via KML",
        Arc::new(pe_trackers::yellowbrick::YellowBrick::at(
            &format!("http://127.0.0.1:{port}"),
            pe_trackers::yellowbrick::SITE,
        )),
        "yb.tl/rmsr2024",
    );
}
