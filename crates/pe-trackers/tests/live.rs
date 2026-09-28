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

use pe_trackers::bluewater::BlueWaterTracks;
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

/// Geovoile end to end: New York Vendée 2024 (the fixture's figures) and
/// leg 1 of the Solitaire du Figaro 2024.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn geovoile_2024_sites() {
    if !live() {
        return;
    }
    let client = pe_trackers::geovoile::Geovoile::default();
    for (input, boats, fixes) in [
        (
            "https://newyorkvendee.geovoile.com/2024/tracker/",
            28,
            64_453,
        ),
        (
            "https://lasolitaire.geovoile.com/2024/tracker/?leg=1",
            45,
            21_990,
        ),
    ] {
        let start = Instant::now();
        let event = client.resolve(input).expect("resolves");
        let mut last = 0.0;
        let event = client
            .fetch(&event, &fetcher(), &mut |p| last = p.fraction())
            .expect("fetches");
        let count: usize = event.boats.iter().map(|b| b.fixes.len()).sum();
        let official = event
            .boats
            .iter()
            .flat_map(|b| &b.fixes)
            .filter(|f| f.sog.is_some())
            .count();
        println!(
            "M11 | Geovoile {input}: {} boats, {count} fixes ({official} with official speed), {:.2} s",
            event.boats.len(),
            start.elapsed().as_secs_f64()
        );
        assert_eq!(event.boats.len(), boats);
        assert_eq!(count, fixes);
        assert!((last - 1.0).abs() < 1e-9, "progress ends at 1, got {last}");
    }
}

/// The 2025 Melbourne Hobart Westcoaster end to end through the client,
/// against the recorded figures.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn bluewater_melbourne_hobart_2025() {
    if !live() {
        return;
    }
    let client = BlueWaterTracks::default();
    let event = client
        .resolve("https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster")
        .expect("resolves");
    let fetcher = Fetcher::new("Blue Water Tracks", Duration::from_secs(60), Arc::default())
        .expect("a client");
    let start = Instant::now();
    let mut last = 0.0;
    let event = client
        .fetch(&event, &fetcher, &mut |p| last = p.fraction())
        .expect("fetches");
    let fixes: usize = event.boats.iter().map(|b| b.fixes.len()).sum();
    println!(
        "M12 | Blue Water Tracks melbourne-hobart-2025 live: {} boats, {fixes} fixes, {:.2} s",
        event.boats.len(),
        start.elapsed().as_secs_f64()
    );
    assert_eq!(event.title, "2025 Melbourne Hobart Westcoaster");
    assert_eq!(event.boats.len(), 5);
    assert!((last - 1.0).abs() < 1e-9, "progress ends at 1, got {last}");
}

/// An unknown slug: `race` comes back an empty array, which the client
/// reads as no public event, never as a decode failure.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn bluewater_unknown_slug() {
    if !live() {
        return;
    }
    let client = BlueWaterTracks::default();
    let event = client.resolve("no-such-race-xyz-123").expect("a slug");
    let fetcher = Fetcher::new("Blue Water Tracks", Duration::from_secs(60), Arc::default())
        .expect("a client");
    let err = client
        .fetch(&event, &fetcher, &mut |_| {})
        .expect_err("no such event");
    println!("M12 | unknown slug: {err}");
    assert!(
        matches!(err, pe_trackers::TrackerError::NoSuchEvent { .. }),
        "{err:?}"
    );
}
