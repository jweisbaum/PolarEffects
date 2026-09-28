#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Decoders against recorded vendor responses (plan.md Appendices A and B).
//!
//! # How the fixtures were recorded (2026-09-27/28)
//!
//! - `yellowbrick/fastnet2025-RaceSetup.json`: `cf.yb.tl/JSON/fastnet2025/
//!   RaceSetup` as served (ISO-8859-1). `…-AllPositions3-first65.bin`: the
//!   first 786 KB of `cf.yb.tl/BIN/fastnet2025/AllPositions3` (5.7 MB, 444
//!   teams), cut at the end of the 65th team so it is itself a valid file.
//! - `yellowbrick/rmsr2024-RaceSetup.json` (2026-09-28): the Rolex Middle Sea
//!   Race 2024 setup as served. `…-AllPositions3-first3.bin`: the first
//!   44,889 bytes of its `AllPositions3` (1.37 MB, 112 teams), cut at the
//!   end of the third team. `rmsr2024-3teams.kml`: `yb.tl/rmsr2024.kml`
//!   (23 MB) with the placemarks of the same three teams kept and the rest
//!   cut out; header and styles untouched.
//! - `geovoile/24hultim2025/`: the viewer page and the config, tracks and
//!   reports resources of `24hultim.geovoile.com/2025/tracker/`.
//! - `geovoile/routedurhum2022/viewer.html`: the viewer page only, for the
//!   seed parser.
//! - `geovoile/vendeeglobe2016/`: the versions, config and tracks resources
//!   of `vendeeglobe.geovoile.com/2016/tracker/`. Its viewer page answers HTTP
//!   500 as of 2026-09-28, so its seeds are the ones recorded in plan.md
//!   Appendix A during research.
//!
//! Reference values come from the Python reference decoder written during
//! research (an independent implementation of Appendix A) and from facts
//! about the races: the Fastnet started from Cowes on 2025-07-26 and
//! finishes at Cherbourg; the 24 Heures Ultim started at Lorient on
//! 2025-09-27; the Vendée Globe 2016 started at Les Sables-d'Olonne on
//! 2016-11-06 at 12:02Z with 29 boats.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pe_trackers::event::PositionsFrom;
use pe_trackers::geovoile::{self, Seeds};
use pe_trackers::yellowbrick::{self, YellowBrick};
use pe_trackers::{Fetcher, TrackerClient, TrackerError};

fn fixture(path: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(path),
    )
    .expect(path)
}

fn fixture_text(path: &str) -> String {
    String::from_utf8(fixture(path)).expect("UTF-8")
}

// --- YellowBrick -----------------------------------------------------------

#[test]
fn fastnet_race_setup_decodes_latin1() {
    let setup = yellowbrick::parse_race_setup(&fixture("yellowbrick/fastnet2025-RaceSetup.json"))
        .expect("parses");
    assert_eq!(setup.title, "Rolex Fastnet 2025");
    assert_eq!(setup.teams.len(), 444);
    // 2025-07-26T09:30Z, the first start.
    assert_eq!(setup.start, Some(1_753_522_200));
    let team1 = setup.teams.iter().find(|t| t.id == 1).expect("team 1");
    assert_eq!(team1.name, "BE WATER POSITIVE");
    assert_eq!(team1.sail.as_deref(), Some("CAN80"));
    assert_eq!(team1.model.as_deref(), Some("IMOCA60"));
    assert!(
        setup
            .teams
            .iter()
            .any(|t| t.owner.as_deref() == Some("Jean-François Cheriaux")),
        "a Latin-1 name survives"
    );
}

#[test]
fn fastnet_all_positions_decode_to_cowes_and_cherbourg() {
    let all = yellowbrick::decode_all_positions(&fixture(
        "yellowbrick/fastnet2025-AllPositions3-first65.bin",
    ))
    .expect("decodes");
    assert_eq!(all.flags, 0x02, "distance to finish only");
    assert_eq!(all.ref_time, 1_720_000_000);
    assert_eq!(all.teams.len(), 65);
    assert_eq!(
        all.teams.iter().map(|t| t.moments.len()).sum::<usize>(),
        98_042
    );

    let team1 = &all.teams[0];
    assert_eq!(team1.id, 1);
    assert_eq!(team1.moments.len(), 550);
    let first = team1.moments[0];
    // 2025-07-26T09:32:00Z in the Solent off Cowes.
    assert_eq!(first.at, 1_753_522_320);
    assert_eq!((first.lat, first.lon), (50.764_59, -1.223_61));
    assert_eq!(first.dtf, Some(1_301_104));
    let last = *team1.moments.last().expect("fixes");
    // Retired (RaceSetup "Technical Issue") and stopped at Cherbourg.
    assert_eq!(last.at, 1_753_660_818);
    assert_eq!((last.lat, last.lon), (49.638_3, -1.621_21));

    // Oldest first, never going backwards, all within the race area.
    for team in &all.teams {
        assert!(
            team.moments.windows(2).all(|w| w[0].at <= w[1].at),
            "team {}",
            team.id
        );
        for m in &team.moments {
            assert!(
                (43.0..53.0).contains(&m.lat) && (-12.0..3.0).contains(&m.lon),
                "{m:?}"
            );
        }
    }
    let fixes = team1.fixes();
    assert_eq!(fixes.len(), 550);
    assert_eq!(
        (fixes[0].t, fixes[0].lat, fixes[0].lon),
        (1_753_522_320, 50.764_59, -1.223_61)
    );
    assert!(fixes.iter().all(|f| f.cog.is_none() && f.sog.is_none()));
}

/// A file cut inside a team is an error naming the byte, not a short event.
#[test]
fn a_truncated_all_positions_is_refused() {
    let bytes = fixture("yellowbrick/fastnet2025-AllPositions3-first65.bin");
    let err = yellowbrick::decode_all_positions(&bytes[..bytes.len() - 3]).expect_err("truncated");
    assert!(err.to_string().contains("byte"), "{err}");
}

/// The whole 5.7 MB Fastnet file, when a copy is at `PE_YB_FULL`: the
/// Appendix B facts (444 teams, every byte consumed) and the decode time.
#[test]
#[ignore = "needs the full recording at PE_YB_FULL"]
fn the_full_fastnet_file_decodes_and_is_fast() {
    let Ok(path) = std::env::var("PE_YB_FULL") else {
        return;
    };
    let bytes = std::fs::read(path).expect("readable");
    assert_eq!(bytes.len(), 5_726_173);
    let start = std::time::Instant::now();
    let all = yellowbrick::decode_all_positions(&bytes).expect("decodes");
    let elapsed = start.elapsed();
    let fixes: usize = all.teams.iter().map(|t| t.moments.len()).sum();
    println!(
        "M3 | AllPositions3 fastnet2025: {} teams, {fixes} fixes, decoded in {:.1} ms",
        all.teams.len(),
        elapsed.as_secs_f64() * 1e3
    );
    assert_eq!(all.teams.len(), 444);
}

#[test]
fn divisions_come_from_the_starting_tags() {
    let setup = yellowbrick::parse_race_setup(&fixture("yellowbrick/fastnet2025-RaceSetup.json"))
        .expect("parses");
    let team = |id| setup.teams.iter().find(|t| t.id == id).expect("a team");
    // Tagged "Line Honours Monohull", "IRC Overall" and "IRC 2"; only
    // "IRC 2" has a start of its own.
    assert_eq!(setup.division(team(3)).as_deref(), Some("IRC 2"));
    assert_eq!(setup.division(team(1)).as_deref(), Some("IMOCA"));
    assert_eq!(team(3).start, Some(1_753_530_000));
    assert_eq!(team(3).finished_at, Some(1_753_916_050));
    assert_eq!(setup.stop, Some(1_754_110_800));

    // No tag of the Middle Sea Race 2024 has its own start: all shown tags
    // ("Group 6" is hidden, `show: 0`).
    let setup = yellowbrick::parse_race_setup(&fixture("yellowbrick/rmsr2024-RaceSetup.json"))
        .expect("parses");
    assert_eq!(setup.title, "Rolex Middle Sea Race 2024");
    assert_eq!(setup.teams.len(), 112);
    let first = &setup.teams[0];
    assert_eq!(first.name, "12 NACIRA 69");
    assert_eq!(
        setup.division(first).as_deref(),
        Some("Line Honours Monohull, IRC Overall, IRC Class 2")
    );
}

/// The KML fallback against the binary of the same race: every KML
/// position is one the binary has, at the same time, on the same 1e-5
/// grid. The binary also holds a few reports at a repeated time, which the
/// KML leaves out (1689 against 1670 for the first team).
#[test]
fn rmsr_kml_matches_the_binary() {
    let kml =
        pe_trackers::kml::parse_tracks(&fixture("yellowbrick/rmsr2024-3teams.kml")).expect("reads");
    let names: Vec<&str> = kml.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(names, ["12 NACIRA 69", "AFAZIK IMPULSE", "ALMAR"]);
    let counts: Vec<usize> = kml.iter().map(|t| t.fixes.len()).collect();
    assert_eq!(counts, [1670, 1684, 2216]);
    let first = &kml[0].fixes[0];
    // 2024-10-18T06:33:25Z at 35.90196N 14.50192E, in Marsamxett Harbour.
    assert_eq!(
        (first.t, first.lat, first.lon),
        (1_729_233_205, 35.901_96, 14.501_92)
    );
    let last = kml[0].fixes.last().expect("fixes");
    assert_eq!(
        (last.t, last.lat, last.lon),
        (1_729_731_600, 35.902, 14.5018)
    );

    let all = yellowbrick::decode_all_positions(&fixture(
        "yellowbrick/rmsr2024-AllPositions3-first3.bin",
    ))
    .expect("decodes");
    assert_eq!(all.teams.len(), 3);
    let counts: Vec<usize> = all.teams.iter().map(|t| t.moments.len()).collect();
    assert_eq!(counts, [1689, 1692, 2225]);
    for (k, team) in all.teams.iter().enumerate() {
        let binary = team.fixes();
        for fix in &kml[k].fixes {
            assert!(
                binary
                    .iter()
                    .any(|b| b.t == fix.t && b.lat == fix.lat && b.lon == fix.lon),
                "{} {fix:?}",
                kml[k].name
            );
        }
    }
}

/// A local server answering by path from recorded responses.
fn serve(routes: Vec<(&'static str, &'static str, Vec<u8>)>) -> String {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let port = listener.local_addr().expect("an address").port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
            let (status, body) = routes
                .iter()
                .find(|(p, _, _)| *p == path)
                .map_or(("404 Not Found", Vec::new()), |(_, s, b)| (*s, b.clone()));
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

fn fetch_rmsr(
    positions: (&'static str, Vec<u8>),
) -> pe_trackers::Result<pe_trackers::TrackerEvent> {
    let host = serve(vec![
        (
            "/JSON/rmsr2024/RaceSetup",
            "200 OK",
            fixture("yellowbrick/rmsr2024-RaceSetup.json"),
        ),
        ("/BIN/rmsr2024/AllPositions3", positions.0, positions.1),
        (
            "/rmsr2024.kml",
            "200 OK",
            fixture("yellowbrick/rmsr2024-3teams.kml"),
        ),
    ]);
    let client = YellowBrick::at(&host, &host);
    let event = client.resolve("https://yb.tl/rmsr2024").expect("resolves");
    let fetcher = Fetcher::new("YellowBrick", Duration::from_secs(10), Arc::default())
        .expect("a client")
        .with_backoff(Duration::from_millis(1));
    let mut last = 0.0;
    let out = client.fetch(&event, &fetcher, &mut |p| last = p.fraction());
    if out.is_ok() {
        assert!((last - 1.0).abs() < 1e-9, "progress ends at 1, got {last}");
    }
    out
}

/// The whole event through the client, from recorded responses: the
/// setup's 112 boats with their sail numbers, models and divisions, the
/// first three with the binary's positions.
#[test]
fn an_event_downloads_through_the_client() {
    let event = fetch_rmsr((
        "200 OK",
        fixture("yellowbrick/rmsr2024-AllPositions3-first3.bin"),
    ))
    .expect("fetches");
    assert_eq!(event.title, "Rolex Middle Sea Race 2024");
    assert_eq!(event.event.url, "https://yb.tl/rmsr2024");
    assert_eq!(event.positions_from, PositionsFrom::Primary);
    assert_eq!(event.boats.len(), 112);
    let boat = event.boat("1").expect("team 1");
    assert_eq!(boat.name, "12 NACIRA 69");
    assert_eq!(boat.sail.as_deref(), Some("ITA17498"));
    assert_eq!(boat.model.as_deref(), Some("NACIRA V69 4.25"));
    assert_eq!(boat.status.as_deref(), Some("FINISHED"));
    assert_eq!(
        (boat.start, boat.finish),
        (Some(1_729_332_000), Some(1_729_722_454))
    );
    assert_eq!(boat.fixes.len(), 1689);
    assert_eq!(event.fixes("3").map(<[_]>::len), Some(2225));
    assert_eq!(
        event.fixes("4").map(<[_]>::len),
        Some(0),
        "not in the cropped binary"
    );
}

/// An `AllPositions3` that is a web page (what YellowBrick answers for a
/// key it has no positions for) falls back to the KML, matched by name.
#[test]
fn a_binary_that_does_not_decode_falls_back_to_the_kml() {
    let event =
        fetch_rmsr(("200 OK", b"\n\n<!DOCTYPE html><html></html>".to_vec())).expect("fetches");
    assert_eq!(event.positions_from, PositionsFrom::Fallback);
    let counts: Vec<usize> = event.boats[..4].iter().map(|b| b.fixes.len()).collect();
    assert_eq!(counts, [1670, 1684, 2216, 0]);
}

/// A tracker that keeps answering 5xx is reported as not answering, for
/// the dialog's Retry, and the KML is not tried.
#[test]
fn a_failing_tracker_is_unavailable_not_a_fallback() {
    let err = fetch_rmsr(("503 Service Unavailable", Vec::new())).expect_err("fails");
    assert!(matches!(err, TrackerError::Unavailable { .. }), "{err:?}");
}

// --- Geovoile ----------------------------------------------------------------

const MODERN: Seeds = Seeds([0x7B_C495, 0x45_57FA, 0xD5_6AAF, 0xFF_8040]);
const VG2016: Seeds = Seeds([0x88_FE88, 0xFE_88AA, 0xEE_CC80, 0xA0_A0F0]);

/// The 2022–2025 seeds as Appendix A records them, parsed from two sites'
/// pages, in `x, y, z, w` order.
#[test]
fn seeds_are_parsed_from_two_viewer_pages() {
    for page in [
        "geovoile/24hultim2025/viewer.html",
        "geovoile/routedurhum2022/viewer.html",
    ] {
        let seeds = geovoile::seeds_from_html(&fixture_text(page)).expect(page);
        assert_eq!(seeds, MODERN, "{page}");
    }
}

#[test]
fn the_24h_ultim_viewer_parameters() {
    let viewer =
        geovoile::parse_viewer(&fixture_text("geovoile/24hultim2025/viewer.html")).expect("parses");
    assert_eq!(viewer.title, "24h Ultim 2025");
    assert_eq!(viewer.root_url, "/2025/");
    assert_eq!(viewer.resources_url, "");
    assert_eq!((viewer.legs, viewer.leg), (1, 1));
    assert_eq!(
        viewer.resource_path("tracks", 42),
        "/2025/tracker/resources/tracks/v42"
    );
    let rdr = geovoile::parse_viewer(&fixture_text("geovoile/routedurhum2022/viewer.html"))
        .expect("parses");
    assert_eq!(rdr.title, "Route du Rhum 2022");
    assert_eq!(rdr.root_url, "/2022/");
}

#[test]
fn the_24h_ultim_config_and_tracks_decode() {
    let seeds = geovoile::parse_viewer(&fixture_text("geovoile/24hultim2025/viewer.html"))
        .unwrap()
        .seeds;
    let xml = geovoile::decode_text(&fixture("geovoile/24hultim2025/config.hwx"), seeds, true)
        .expect("config");
    assert_eq!(xml.len(), 12_219, "the reference decoder's length");
    let config = geovoile::parse_config(&xml).expect("parses");
    assert_eq!(config.name, "24H Ultim");
    assert_eq!(config.date.as_deref(), Some("2025-09-27T10:00:00Z"));
    assert_eq!(config.boats.len(), 14);
    assert_eq!(config.boats[0].id, 1);
    assert_eq!(config.boats[0].name, "Actual Ultim 4");
    assert_eq!(config.boats[0].colour.as_deref(), Some("#dd272f"));
    assert_eq!(config.boats[1].name, "Banque Populaire XI");

    let json = geovoile::decode_text(&fixture("geovoile/24hultim2025/tracks.hwx"), seeds, false)
        .expect("tracks");
    let tracks = geovoile::parse_tracks(&json).expect("parses");
    // Fifteen tracks, of which id 0 is not a boat (a flat list of numbers).
    assert_eq!(tracks.len(), 14);
    assert!(tracks.iter().all(|t| t.id != 0));
    assert_eq!(
        tracks.iter().map(|t| t.fixes.len()).sum::<usize>(),
        8018 - 5
    );
    let boat1 = tracks.iter().find(|t| t.id == 1).expect("boat 1");
    assert_eq!(boat1.fixes.len(), 601);
    let (first, last) = (&boat1.fixes[0], &boat1.fixes[600]);
    // 2025-09-27T09:55:38Z off Lorient, before the 10:00Z start.
    assert_eq!(
        (first.t, first.lat, first.lon),
        (1_758_966_938, 47.693_63, -3.452_9)
    );
    assert_eq!(
        (last.t, last.lat, last.lon),
        (1_759_073_046, 47.546_81, -3.380_45)
    );

    let reports =
        geovoile::decode_text(&fixture("geovoile/24hultim2025/reports.hwx"), seeds, false)
            .expect("reports");
    assert_eq!(reports.len(), 950_682);
    let value: serde_json::Value = serde_json::from_str(&reports).expect("reports are JSON");
    assert!(value["reports"].is_object());
}

/// The oldest supported generation: a 2016 site with its own seeds.
#[test]
fn the_vendee_globe_2016_decodes_with_its_own_seeds() {
    let versions =
        geovoile::parse_versions(&fixture_text("geovoile/vendeeglobe2016/versions.txt")).unwrap();
    assert_eq!(versions["tracks"], 20_220_308_182_529);

    let xml = geovoile::decode_text(
        &fixture("geovoile/vendeeglobe2016/config.hwx"),
        VG2016,
        true,
    )
    .expect("config");
    assert_eq!(xml.len(), 85_985);
    let config = geovoile::parse_config(&xml).expect("parses");
    assert_eq!(config.name, "Vendée Globe 2016");
    assert_eq!(config.boats.len(), 29);
    assert!(
        config
            .boats
            .iter()
            .any(|b| b.name == "StMichel - Virbac" && b.sail.as_deref() == Some("FRA06"))
    );

    let json = geovoile::decode_text(
        &fixture("geovoile/vendeeglobe2016/tracks.hwx"),
        VG2016,
        false,
    )
    .expect("tracks");
    let tracks = geovoile::parse_tracks(&json).expect("parses");
    // Thirty tracks: the 29 boats of the config and the non-boat id 0.
    assert_eq!(tracks.len(), 29);
    assert_eq!(
        tracks.iter().map(|t| t.fixes.len()).sum::<usize>(),
        107_588 - 129
    );
    let mut ids: Vec<u32> = tracks.iter().map(|t| t.id).collect();
    let mut boat_ids: Vec<u32> = config.boats.iter().map(|b| b.id).collect();
    ids.sort_unstable();
    boat_ids.sort_unstable();
    assert_eq!(ids, boat_ids, "every track is a configured boat");
    let boat1 = tracks.iter().find(|t| t.id == 1).expect("boat 1");
    let first = &boat1.fixes[0];
    // The start: 2016-11-06T12:02Z off Les Sables-d'Olonne.
    assert_eq!(
        (first.t, first.lat, first.lon),
        (1_478_433_720, 46.424_02, -1.785_05)
    );
    let last = boat1.fixes.last().unwrap();
    assert_eq!(
        (last.t, last.lat, last.lon),
        (1_487_605_680, 46.502_6, -1.788_8)
    );
}

/// The wrong seeds decode to noise, which is refused as an unsupported
/// version rather than parsed.
#[test]
fn the_wrong_seeds_are_refused_not_imported() {
    let err = geovoile::decode_text(
        &fixture("geovoile/vendeeglobe2016/tracks.hwx"),
        MODERN,
        false,
    )
    .expect_err("wrong seeds");
    assert!(
        err.to_string().contains("unsupported Geovoile version"),
        "{err}"
    );
    let err = geovoile::decode_text(&fixture("geovoile/24hultim2025/config.hwx"), VG2016, true)
        .expect_err("wrong seeds");
    assert!(
        err.to_string().contains("unsupported Geovoile version"),
        "{err}"
    );
}

#[test]
fn a_truncated_hwx_is_an_error_naming_the_byte() {
    let bytes = fixture("geovoile/24hultim2025/tracks.hwx");
    let err = geovoile::decode_hwx(&bytes[..bytes.len() / 2], MODERN).expect_err("truncated");
    assert!(err.to_string().contains("byte"), "{err}");
}
