#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Live requests to the trackers. Never in the default suite: `#[ignore]`
//! and gated on `PE_TEST_LIVE=1`.
//!
//! ```text
//! PE_TEST_LIVE=1 cargo test -p pe-trackers --test live -- --ignored --nocapture
//! ```

use std::sync::Arc;
use std::time::{Duration, Instant};

use pe_trackers::event::PositionsFrom;
use pe_trackers::yellowbrick::{self, YellowBrick};
use pe_trackers::{Fetcher, TrackerClient};

fn live() -> bool {
    std::env::var("PE_TEST_LIVE").is_ok_and(|v| v == "1")
}

fn fetcher() -> Fetcher {
    Fetcher::new("YellowBrick", Duration::from_secs(60), Arc::default()).expect("a client")
}

/// The Fastnet 2025 event end to end through the client, as Appendix B
/// recorded it.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn yellowbrick_fastnet_2025() {
    if !live() {
        return;
    }
    let start = Instant::now();
    let event = YellowBrick::default()
        .resolve("https://yb.tl/fastnet2025")
        .expect("resolves");
    let mut last = 0.0;
    let event = YellowBrick::default()
        .fetch(&event, &fetcher(), &mut |p| last = p.fraction())
        .expect("fetches");
    let fixes: usize = event.boats.iter().map(|b| b.fixes.len()).sum();
    println!(
        "M10 | YellowBrick fastnet2025 live: {} boats, {fixes} fixes, {:.2} s",
        event.boats.len(),
        start.elapsed().as_secs_f64()
    );
    assert_eq!(event.title, "Rolex Fastnet 2025");
    assert_eq!(event.boats.len(), 444);
    assert_eq!(fixes, 714_380);
    assert_eq!(event.positions_from, PositionsFrom::Primary);
    assert!((last - 1.0).abs() < 1e-9, "progress ends at 1, got {last}");
}

/// The KML fallback as served: Middle Sea Race 2024 (23 MB, built on
/// request in about 70 s), every placemark matched to a RaceSetup team.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn yellowbrick_kml_rmsr_2024() {
    if !live() {
        return;
    }
    let start = Instant::now();
    let fetcher = fetcher();
    let kml = fetcher
        .get_with_timeout(
            &format!("{}/rmsr2024.kml", yellowbrick::SITE),
            yellowbrick::KML_TIMEOUT,
            &mut |_, _| {},
        )
        .expect("the KML");
    let tracks = pe_trackers::kml::parse_tracks(&kml).expect("reads");
    println!(
        "M10 | YellowBrick rmsr2024 KML live: {} bytes, {} placemarks, {:.1} s",
        kml.len(),
        tracks.len(),
        start.elapsed().as_secs_f64()
    );
    assert_eq!(tracks.len(), 112);
    assert_eq!(tracks[0].name, "12 NACIRA 69");
    assert_eq!(tracks[0].fixes.len(), 1670);
}

/// An unknown key: RaceSetup answers 500, which is retried and then
/// reported as the tracker not answering (the dialog offers Retry).
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn yellowbrick_unknown_key() {
    if !live() {
        return;
    }
    let event = YellowBrick::default()
        .resolve("nosuchrace99x")
        .expect("a key");
    let err = YellowBrick::default()
        .fetch(&event, &fetcher(), &mut |_| {})
        .expect_err("no such event");
    println!("M10 | unknown key: {err}");
    assert!(
        matches!(
            err,
            pe_trackers::TrackerError::Unavailable { .. }
                | pe_trackers::TrackerError::NoSuchEvent { .. }
        ),
        "{err:?}"
    );
}
