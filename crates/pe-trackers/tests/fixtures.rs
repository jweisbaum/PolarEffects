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
//! - `geovoile/vendeeglobe2016/`: the versions, config, tracks and (M11)
//!   reports resources of `vendeeglobe.geovoile.com/2016/tracker/`. Its
//!   viewer page answers HTTP 500 as of 2026-09-28, so its seeds are the ones
//!   recorded in plan.md Appendix A during research.
//! - M11 (2026-09-28): `24hultim2025/versions.txt`;
//!   `routedurhum2018/` (viewer, an empty versions file as served, config,
//!   tracks); `newyorkvendee2024/` and `lasolitaire2024-leg1/` (viewer,
//!   versions, config, tracks; the Solitaire's page and resources are leg 1
//!   of 3, `…/2024/tracker/?leg=1` and `resources/leg1/…`);
//!   `routedurhum2014/viewer.html`, the 2012–2015 generation's page, which
//!   is refused.
//! - M12 (2026-09-28): `bluewater/melbournehobartwestcoaster2025-race.json`:
//!   `api.bluewatertracks.com/api/race/2025-melbourne-hobart-westcoaster` as
//!   served, with crew, bios, images and sponsor details cropped out (the
//!   fields the client reads, and the "other known fields" spec.md 7.2's
//!   research notes list, kept).
//!
//! Reference values come from the Python reference decoder written during
//! research (an independent implementation of Appendix A, used again in
//! M11 for the new sites, the reports and the report-to-fix matching), from
//! each site's own official arrival times in its reports, and from facts
//! about the races: the Fastnet started from Cowes on 2025-07-26 and
//! finishes at Cherbourg; the 24 Heures Ultim started at Lorient on
//! 2025-09-27; the Vendée Globe 2016 started at Les Sables-d'Olonne on
//! 2016-11-06 at 12:02Z with 29 boats.

use std::io::{Read, Write};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pe_trackers::bluewater::{self, BlueWaterTracks};
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
    serve_recorded(routes).0
}

fn serve_recorded(
    routes: Vec<(&'static str, &'static str, Vec<u8>)>,
) -> (String, Arc<std::sync::Mutex<Vec<String>>>) {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
    let port = listener.local_addr().expect("an address").port();
    let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
    let seen = requests.clone();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let Ok(mut stream) = stream else { break };
            let mut buf = vec![0u8; 8192];
            let n = stream.read(&mut buf).unwrap_or(0);
            let request = String::from_utf8_lossy(&buf[..n]).into_owned();
            let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
            seen.lock().unwrap().push(path.clone());
            // A route ending in `*` matches every path it starts (the
            // Geovoile versions file's cache-busting number).
            let (status, body) = routes
                .iter()
                .find(|(p, _, _)| {
                    p.strip_suffix('*')
                        .map_or(*p == path, |prefix| path.starts_with(prefix))
                })
                .map_or(("404 Not Found", Vec::new()), |(_, s, b)| (*s, b.clone()));
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
    let mut seen = Vec::new();
    let out = client.fetch(&event, &fetcher, &mut |p| seen.push(p.fraction()));
    // Never backwards, the fallback included (M10 review).
    assert!(
        seen.windows(2).all(|w| w[0] <= w[1]),
        "progress goes back: {seen:?}"
    );
    if out.is_ok() {
        let last = seen.last().copied().unwrap_or_default();
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

// --- Geovoile sites of five years (M11 acceptance) ---------------------------

/// Decodes one site's config and tracks with the seeds its own viewer page
/// carries (Appendix A's for the Vendée Globe 2016, whose page is gone).
fn site(dir: &str, seeds: Option<Seeds>) -> (geovoile::Config, Vec<geovoile::Track>) {
    let seeds = seeds.unwrap_or_else(|| {
        geovoile::seeds_from_html(&fixture_text(&format!("geovoile/{dir}/viewer.html")))
            .expect("seeds")
    });
    let xml = geovoile::decode_text(&fixture(&format!("geovoile/{dir}/config.hwx")), seeds, true)
        .expect("config");
    let json = geovoile::decode_text(
        &fixture(&format!("geovoile/{dir}/tracks.hwx")),
        seeds,
        false,
    )
    .expect("tracks");
    (
        geovoile::parse_config(&xml).expect("config parses"),
        geovoile::parse_tracks(&json).expect("tracks parse"),
    )
}

/// Boat count, total fixes, and the first configured boat's first and last
/// fix, per site. The references are the independent Python decoder's; the
/// facts are the races' (each first fix is at its start line or harbour,
/// each last one at the finish port).
#[test]
fn five_sites_of_four_generations_decode_to_the_reference() {
    type Fixed = (i64, f64, f64);
    struct Case {
        dir: &'static str,
        seeds: Option<Seeds>,
        name: &'static str,
        boats: usize,
        fixes: usize,
        first_boat: &'static str,
        count: usize,
        first: Fixed,
        last: Fixed,
    }
    let cases = [
        // Les Sables-d'Olonne 2016-11-06T12:02Z, back there 2017-02-23.
        Case {
            dir: "vendeeglobe2016",
            seeds: Some(VG2016),
            name: "Vendée Globe 2016",
            boats: 29,
            fixes: 107_459,
            first_boat: "One Planet One Ocean",
            count: 5574,
            first: (1_478_433_720, 46.424_02, -1.785_05),
            last: (1_487_869_560, 46.502_6, -1.788_7),
        },
        // Saint-Malo 2018-11-04 to Pointe-à-Pitre.
        Case {
            dir: "routedurhum2018",
            seeds: None,
            name: "Route du Rhum 2018",
            boats: 26,
            fixes: 28_038,
            first_boat: "Trimaran MACIF",
            count: 1099,
            first: (1_541_329_200, 48.786_89, -1.875_94),
            last: (1_542_544_203, 16.206_96, -61.506_41),
        },
        // New York 2024-05-29 to Les Sables-d'Olonne.
        Case {
            dir: "newyorkvendee2024",
            seeds: None,
            name: "New York Vendée 2024",
            boats: 28,
            fixes: 64_453,
            first_boat: "Be Water Positive",
            count: 2485,
            first: (1_716_908_400, 41.490_95, -71.324_25),
            last: (1_718_303_400, 46.503, -1.788_73),
        },
        // Leg 1, Rouen 2024-08-25T13:30Z to Gijón.
        Case {
            dir: "lasolitaire2024-leg1",
            seeds: None,
            name: "Solitaire du Figaro",
            boats: 45,
            fixes: 21_990,
            first_boat: "Actual",
            count: 491,
            first: (1_724_592_600, 49.527_28, 0.008_92),
            last: (1_724_994_000, 43.546_93, -5.668_27),
        },
        // Lorient to Lorient, 2025-09-27.
        Case {
            dir: "24hultim2025",
            seeds: None,
            name: "24H Ultim",
            boats: 14,
            fixes: 8013,
            first_boat: "Actual Ultim 4",
            count: 601,
            first: (1_758_966_938, 47.693_63, -3.452_9),
            last: (1_759_073_046, 47.546_81, -3.380_45),
        },
    ];
    for case in cases {
        let dir = case.dir;
        let (config, tracks) = site(dir, case.seeds);
        assert_eq!(config.name, case.name, "{dir}");
        assert_eq!(config.boats.len(), case.boats, "{dir}");
        assert_eq!(tracks.len(), case.boats, "{dir}: one track per boat");
        assert_eq!(
            tracks.iter().map(|t| t.fixes.len()).sum::<usize>(),
            case.fixes,
            "{dir}"
        );
        let boat = &config.boats[0];
        assert_eq!(boat.name, case.first_boat, "{dir}");
        let track = tracks.iter().find(|t| t.id == boat.id).expect("its track");
        assert_eq!(track.fixes.len(), case.count, "{dir}");
        let at = |f: &pe_core::track::Fix| (f.t, f.lat, f.lon);
        assert_eq!(at(&track.fixes[0]), case.first, "{dir} first");
        assert_eq!(
            at(track.fixes.last().expect("fixes")),
            case.last,
            "{dir} last"
        );
    }
}

/// The page parameters of the newer sites, including the Solitaire's legs.
#[test]
fn viewer_pages_of_2018_2024_give_their_parameters() {
    let v = geovoile::parse_viewer(&fixture_text("geovoile/lasolitaire2024-leg1/viewer.html"))
        .expect("parses");
    assert_eq!((v.root_url.as_str(), v.legs, v.leg), ("/2024/", 3, 1));
    assert_eq!(
        v.resource_path("tracks", 5),
        "/2024/tracker/resources/leg1/tracks/v5"
    );
    let v = geovoile::parse_viewer(&fixture_text("geovoile/newyorkvendee2024/viewer.html"))
        .expect("parses");
    assert_eq!(v.title, "New York - Vendée 2024");
    assert_eq!((v.legs, v.resources_url.as_str()), (1, ""));
    let v = geovoile::parse_viewer(&fixture_text("geovoile/routedurhum2018/viewer.html"))
        .expect("parses");
    assert_eq!(v.root_url, "/2018/");
    // The 2018 page carries the same seeds as 2022–2025.
    assert_eq!(v.seeds, MODERN);
}

/// The Solitaire's leg 1 config: the leg, the class and the start of its
/// run, which becomes each boat's start.
#[test]
fn a_leg_config_gives_its_leg_classes_and_runs() {
    let (config, _) = site("lasolitaire2024-leg1", None);
    assert_eq!(config.leg, Some(geovoile::Leg { num: 1, total: 3 }));
    assert!(!config.classes.is_empty());
    let boat = &config.boats[0];
    let class = config
        .classes
        .iter()
        .find(|c| Some(c.id) == boat.class)
        .expect("its class");
    let run = config
        .runs
        .iter()
        .find(|r| Some(r.id) == class.run)
        .expect("its run");
    assert_eq!(run.start.as_deref(), Some("2024-08-25T13:30:00Z"));
    // The Route du Rhum 2022's six classes (its config is not a fixture;
    // the 2018 one has the same shape).
    let (config, _) = site("routedurhum2018", None);
    assert!(config.boats.iter().all(|b| b.class.is_some()));
}

/// The 2016 reports: their own column order, the official arrivals (Armel
/// Le Cléac'h first, 2017-01-19T15:37:46Z) and the hidden (retired) boats.
#[test]
fn the_vendee_globe_2016_reports_parse_by_column_name() {
    let json = geovoile::decode_text(
        &fixture("geovoile/vendeeglobe2016/reports.hwx"),
        VG2016,
        false,
    )
    .expect("reports");
    let reports = geovoile::parse_reports(&json).expect("parses");
    assert_eq!(reports.lines.len(), 21_779);
    assert_eq!(reports.arrivals.len(), 18);
    assert_eq!(reports.arrivals[&3], 1_484_840_266);
    assert_eq!(reports.hidden.len(), 8);
    assert!(reports.lines.windows(2).all(|w| w[0].t <= w[1].t));
}

/// A local server holding one Geovoile site's recorded responses.
fn geovoile_site(
    page_path: &'static str,
    page: Vec<u8>,
    root: &'static str,
    resources: Vec<(&'static str, &'static str, Vec<u8>)>,
) -> String {
    let mut routes = vec![(page_path, "200 OK", page)];
    let _ = root;
    routes.extend(resources);
    serve(routes)
}

fn fetch_geovoile(
    host: &str,
    input: &str,
) -> (pe_trackers::Result<pe_trackers::TrackerEvent>, Vec<f64>) {
    let client = geovoile::Geovoile::at(host);
    let event = client.resolve(input).expect("resolves");
    let fetcher = Fetcher::new("Geovoile", Duration::from_secs(10), Arc::default())
        .expect("a client")
        .with_backoff(Duration::from_millis(1));
    let mut seen = Vec::new();
    let out = client.fetch(&event, &fetcher, &mut |p| seen.push(p.fraction()));
    assert!(
        seen.windows(2).all(|w| w[0] <= w[1]),
        "progress goes back: {seen:?}"
    );
    (out, seen)
}

fn ultim_routes(page: Vec<u8>) -> String {
    geovoile_site(
        "/2025/tracker/",
        page,
        "/2025/",
        vec![
            (
                "/2025/tracker/resources/versions/v*",
                "200 OK",
                fixture("geovoile/24hultim2025/versions.txt"),
            ),
            (
                "/2025/tracker/resources/config/v20251006074618",
                "200 OK",
                fixture("geovoile/24hultim2025/config.hwx"),
            ),
            (
                "/2025/tracker/resources/tracks/v20250928152939",
                "200 OK",
                fixture("geovoile/24hultim2025/tracks.hwx"),
            ),
            (
                "/2025/tracker/resources/reports/v20250928152939",
                "200 OK",
                fixture("geovoile/24hultim2025/reports.hwx"),
            ),
        ],
    )
}

/// The whole 24 Heures Ultim 2025 through the client from its recorded
/// responses: the reports' official heading and speed on the fixes they
/// describe, their statuses and finish times.
#[test]
fn a_geovoile_event_downloads_through_the_client() {
    let host = ultim_routes(fixture("geovoile/24hultim2025/viewer.html"));
    let (event, seen) = fetch_geovoile(&host, "https://24hultim.geovoile.com/2025/tracker/");
    let event = event.expect("fetches");
    assert!((seen.last().copied().unwrap_or_default() - 1.0).abs() < 1e-9);
    assert_eq!(event.title, "24H Ultim");
    assert_eq!(
        event.event.url,
        "https://24hultim.geovoile.com/2025/tracker/"
    );
    assert_eq!(event.event.key, "24hultim.geovoile.com/2025/");
    assert_eq!(event.leg, None);
    // The config's start, 2025-09-27T10:00Z.
    assert_eq!(event.start, Some(1_758_967_200));
    assert_eq!(event.boats.len(), 14);
    assert_eq!(event.boats[0].name, "Actual Ultim 4");
    assert_eq!(event.boats[0].status.as_deref(), Some("DNF"));
    assert_eq!(event.boats[0].start, Some(1_758_967_200));
    assert_eq!(
        event.boats[0].finish, event.stop,
        "no arrival: the event's end"
    );
    let boat = event.boat("4").expect("boat 4");
    assert_eq!(boat.status.as_deref(), Some("FINISHED"));
    // Its official arrival, 2025-09-28T06:32:52Z.
    assert_eq!(boat.finish, Some(1_759_041_172));
    assert_eq!(
        event.boat("12").and_then(|b| b.status.as_deref()),
        Some("RETIRED")
    );
    // The Python reference: 292 of boat 4's 470 fixes have a report within
    // 60 s; 246 of those have a heading and a speed.
    assert_eq!(boat.fixes.len(), 470);
    assert_eq!(boat.fixes.iter().filter(|f| f.cog.is_some()).count(), 246);
    assert_eq!(boat.fixes.iter().filter(|f| f.sog.is_some()).count(), 246);
    let fix = boat
        .fixes
        .iter()
        .find(|f| f.t == 1_759_002_902)
        .expect("a fix near the 19:55Z report");
    assert_eq!((fix.cog, fix.sog), (Some(56.0), Some(25.5)));
}

/// Leg 1 of the Solitaire du Figaro 2024 (three legs), asked for by
/// `?leg=1`; its reports are not served, so heading and speed are left to
/// be derived.
#[test]
fn one_leg_of_a_race_in_legs_downloads() {
    let host = geovoile_site(
        "/2024/tracker/?leg=1",
        fixture("geovoile/lasolitaire2024-leg1/viewer.html"),
        "/2024/",
        vec![
            (
                "/2024/tracker/resources/leg1/versions/v*",
                "200 OK",
                fixture("geovoile/lasolitaire2024-leg1/versions.txt"),
            ),
            (
                "/2024/tracker/resources/leg1/config/v20240829093141",
                "200 OK",
                fixture("geovoile/lasolitaire2024-leg1/config.hwx"),
            ),
            (
                "/2024/tracker/resources/leg1/tracks/v20240830050344",
                "200 OK",
                fixture("geovoile/lasolitaire2024-leg1/tracks.hwx"),
            ),
        ],
    );
    let (event, seen) = fetch_geovoile(&host, "lasolitaire.geovoile.com/2024/viewer/?leg=1");
    let event = event.expect("fetches");
    assert_eq!(seen.last().copied(), Some(1.0), "done without the reports");
    assert_eq!(event.leg, Some((1, 3)));
    assert_eq!(event.title, "Solitaire du Figaro (1/3)");
    assert_eq!(
        event.event.url,
        "https://lasolitaire.geovoile.com/2024/tracker/?leg=1"
    );
    assert_eq!(event.boats.len(), 45);
    assert_eq!(event.boats[0].start, Some(1_724_592_600));
    assert!(event.boats[0].division.is_some());
    assert!(
        event
            .boats
            .iter()
            .flat_map(|b| &b.fixes)
            .all(|f| f.cog.is_none() && f.sog.is_none())
    );
}

/// The Route du Rhum 2018 answers an empty versions file: version 0 serves.
#[test]
fn an_empty_versions_file_reads_version_zero() {
    let host = geovoile_site(
        "/2018/tracker/",
        fixture("geovoile/routedurhum2018/viewer.html"),
        "/2018/",
        vec![
            ("/2018/tracker/resources/versions/v*", "200 OK", Vec::new()),
            (
                "/2018/tracker/resources/config/v0",
                "200 OK",
                fixture("geovoile/routedurhum2018/config.hwx"),
            ),
            (
                "/2018/tracker/resources/tracks/v0",
                "200 OK",
                fixture("geovoile/routedurhum2018/tracks.hwx"),
            ),
        ],
    );
    let (event, _) = fetch_geovoile(&host, "https://routedurhum.geovoile.com/2018/tracker/");
    let event = event.expect("fetches");
    assert_eq!(event.title, "Route du Rhum 2018");
    assert_eq!(event.boats.len(), 26);
    // 2018-11-04T13:00Z, the start.
    assert_eq!(event.start, Some(1_541_336_400));
}

/// The 2012–2015 generation's page, a page that is not a viewer, and a 404
/// are refused clearly, before any resource is read.
#[test]
fn older_and_missing_trackers_are_refused_clearly() {
    let host = geovoile_site(
        "/2014/tracker/",
        fixture("geovoile/routedurhum2014/viewer.html"),
        "/2014/",
        vec![("/2024/tracker/", "200 OK", b"Not available".to_vec())],
    );
    let (err, _) = fetch_geovoile(&host, "routedurhum.geovoile.com/2014/tracker/");
    let err = err.expect_err("2014");
    assert!(matches!(err, TrackerError::Legacy { .. }), "{err:?}");
    let (err, _) = fetch_geovoile(&host, "vendeeglobe.geovoile.com/2024/tracker/");
    assert!(
        matches!(err, Err(TrackerError::NoSuchEvent { .. })),
        "{err:?}"
    );
    let (err, _) = fetch_geovoile(&host, "vendeeglobe.geovoile.com/2020/tracker/");
    assert!(
        matches!(err, Err(TrackerError::NoSuchEvent { .. })),
        "{err:?}"
    );
}

/// `resourcesurl` comes from the page, so it is checked against the
/// allow-list before any request: another host, a look-alike, a user name
/// or plain HTTP is refused; a Geovoile host is read (M3 deferred item).
#[test]
fn a_resources_host_off_the_allow_list_is_refused() {
    let page = fixture_text("geovoile/24hultim2025/viewer.html");
    assert!(page.contains("resourcesurl :''"));
    for bad in [
        "https://evil.invalid/r/",
        "https://static.geovoile.com.evil.invalid/r/",
        // A user name before a Geovoile host (built so the offline check
        // reads the host as written).
        &format!("{}x@static.geovoile.com/r/", "https://"),
        "http://static.geovoile.com/r/",
        "//evil.invalid/r/",
    ] {
        let tampered = page.replace("resourcesurl :''", &format!("resourcesurl :'{bad}'"));
        let host = ultim_routes(tampered.into_bytes());
        let (err, _) = fetch_geovoile(&host, "https://24hultim.geovoile.com/2025/tracker/");
        let err = err.expect_err(bad);
        assert!(
            matches!(err, TrackerError::Unsupported { .. })
                && err.to_string().contains("not a Geovoile address"),
            "{bad}: {err}"
        );
    }
    // A static host that is Geovoile's: the viewer's `tracker_<type>.hwx`.
    let tampered = page.replace(
        "resourcesurl :''",
        "resourcesurl :'https://static.geovoile.com/24h/'",
    );
    let host = geovoile_site(
        "/2025/tracker/",
        tampered.into_bytes(),
        "/2025/",
        vec![
            (
                "/24h/tracker_versions.hwx*",
                "200 OK",
                fixture("geovoile/24hultim2025/versions.txt"),
            ),
            (
                "/24h/tracker_config.hwx?v=20251006074618",
                "200 OK",
                fixture("geovoile/24hultim2025/config.hwx"),
            ),
            (
                "/24h/tracker_tracks.hwx?v=20250928152939",
                "200 OK",
                fixture("geovoile/24hultim2025/tracks.hwx"),
            ),
        ],
    );
    let (event, _) = fetch_geovoile(&host, "https://24hultim.geovoile.com/2025/tracker/");
    assert_eq!(event.expect("fetches").boats.len(), 14);
}

/// Resources that do not decode with the page's seeds (here the 2016
/// site's config served by the 2025 site) are an unsupported version, never
/// garbage boats.
#[test]
fn resources_that_do_not_match_the_seeds_are_an_unsupported_version() {
    let host = geovoile_site(
        "/2025/tracker/",
        fixture("geovoile/24hultim2025/viewer.html"),
        "/2025/",
        vec![
            (
                "/2025/tracker/resources/versions/v*",
                "200 OK",
                fixture("geovoile/24hultim2025/versions.txt"),
            ),
            (
                "/2025/tracker/resources/config/v20251006074618",
                "200 OK",
                fixture("geovoile/vendeeglobe2016/config.hwx"),
            ),
        ],
    );
    let (err, _) = fetch_geovoile(&host, "https://24hultim.geovoile.com/2025/tracker/");
    let err = err.expect_err("wrong seeds");
    assert!(
        err.to_string().contains("unsupported Geovoile version"),
        "{err}"
    );
}

// --- Blue Water Tracks -------------------------------------------------------

/// The recorded response decodes to the race and its boats.
#[test]
fn melbourne_hobart_2025_race_decodes() {
    let response = bluewater::parse_race(&fixture(
        "bluewater/melbournehobartwestcoaster2025-race.json",
    ))
    .expect("parses")
    .expect("a real event");
    assert_eq!(response.race.race_name, "2025 Melbourne Hobart Westcoaster");
    assert_eq!(response.race.boats.len(), 5);
    let alien = response
        .race
        .boats
        .iter()
        .find(|b| b.boat_id == "5631641c671f0c130e6f09b6")
        .expect("Alien");
    assert_eq!(alien.boat_name, "ALIEN");
    assert_eq!(alien.sail_no.as_deref(), Some("R880"));
    assert_eq!(alien.design.as_deref(), Some("Lidgard 36"));
    assert_eq!(alien.handicaps.len(), 3);
}

/// The whole event through the client, from a recorded response: every
/// boat's fixes, sorted (this race's feed happens to already be sorted per
/// boat) and each with its own SOG and COG, given rather than derived.
#[test]
fn a_bluewater_event_downloads_through_the_client() {
    let host = serve(vec![(
        "/api/race/2025-melbourne-hobart-westcoaster",
        "200 OK",
        fixture("bluewater/melbournehobartwestcoaster2025-race.json"),
    )]);
    let client = BlueWaterTracks::at(&host);
    let event = client
        .resolve("https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster")
        .expect("resolves");
    let fetcher = Fetcher::new("Blue Water Tracks", Duration::from_secs(10), Arc::default())
        .expect("a client")
        .with_backoff(Duration::from_millis(1));
    let mut seen = Vec::new();
    let event = client
        .fetch(&event, &fetcher, &mut |p| seen.push(p.fraction()))
        .expect("fetches");
    assert!(seen.windows(2).all(|w| w[0] <= w[1]), "{seen:?}");
    assert!((seen.last().copied().unwrap_or_default() - 1.0).abs() < 1e-9);
    assert_eq!(event.title, "2025 Melbourne Hobart Westcoaster");
    assert_eq!(event.positions_from, PositionsFrom::Primary);
    // 2025-12-27T02:30:00Z, and the tracked window's end.
    assert_eq!(event.start, Some(1_766_802_600));
    assert_eq!(event.stop, Some(1_769_860_800));
    assert_eq!(event.boats.len(), 5);

    let alien = event.boat("5631641c671f0c130e6f09b6").expect("Alien");
    assert_eq!(alien.name, "ALIEN");
    assert_eq!(alien.sail.as_deref(), Some("R880"));
    assert_eq!(alien.model.as_deref(), Some("Lidgard 36"));
    assert_eq!(alien.division.as_deref(), Some("1"));
    assert_eq!(alien.status.as_deref(), Some("Racing"));
    // Its own finishTime, not the event's tracked end.
    assert_eq!(alien.finish, Some(1_767_082_361));
    assert_eq!(alien.fixes.len(), 287);
    let first = &alien.fixes[0];
    assert_eq!(first.t, 1_766_735_775);
    assert_eq!((first.lat, first.lon), (-38.261_605, 144.667_088));
    assert_eq!((first.sog, first.cog), (Some(0.0), Some(0.0)));
    let last = alien.fixes.last().expect("fixes");
    assert_eq!(last.t, 1_769_024_700);
    assert_eq!((last.sog, last.cog), (Some(1.0), Some(90.0)));
    // Oldest first, and every fix given a SOG and a COG (spec.md 7.2: both
    // are given and used, never derived).
    for boat in &event.boats {
        assert!(
            boat.fixes.windows(2).all(|w| w[0].t < w[1].t),
            "{}",
            boat.name
        );
        assert!(
            boat.fixes
                .iter()
                .all(|f| f.sog.is_some() && f.cog.is_some()),
            "{}",
            boat.name
        );
    }
    let total: usize = event.boats.iter().map(|b| b.fixes.len()).sum();
    assert_eq!(total, 863);
}

/// An unknown slug's `{"positions":[],"race":[]}` (`race` an empty array,
/// not the object a real event answers with) is refused as no public
/// event, not decoded as garbage.
#[test]
fn a_bluewater_unknown_slug_is_no_such_event() {
    let host = serve(vec![(
        "/api/race/no-such-race",
        "200 OK",
        br#"{"positions":[],"race":[]}"#.to_vec(),
    )]);
    let client = BlueWaterTracks::at(&host);
    let event = client
        .resolve("https://race.bluewatertracks.com/no-such-race")
        .expect("resolves");
    let fetcher = Fetcher::new("Blue Water Tracks", Duration::from_secs(10), Arc::default())
        .expect("a client")
        .with_backoff(Duration::from_millis(1));
    let err = client
        .fetch(&event, &fetcher, &mut |_| {})
        .expect_err("no such event");
    assert!(matches!(err, TrackerError::NoSuchEvent { .. }), "{err:?}");
}

/// A tracker that answers a plain 404 (a redirect, or a changed API) is
/// refused the same way as the empty-array answer.
#[test]
fn a_bluewater_plain_404_is_also_no_such_event() {
    let host = serve(vec![("/api/race/gone", "404 Not Found", Vec::new())]);
    let client = BlueWaterTracks::at(&host);
    let event = client
        .resolve("https://race.bluewatertracks.com/gone")
        .expect("resolves");
    let fetcher = Fetcher::new("Blue Water Tracks", Duration::from_secs(10), Arc::default())
        .expect("a client")
        .with_backoff(Duration::from_millis(1));
    let err = client
        .fetch(&event, &fetcher, &mut |_| {})
        .expect_err("no such event");
    assert!(matches!(err, TrackerError::NoSuchEvent { .. }), "{err:?}");
}

/// The boat list goes ahead of the positions (D24): YellowBrick hands the
/// setup's boats, without fixes, to `listed` once, before the positions,
/// and the event then carries them with their fixes.
#[test]
fn yellowbrick_lists_the_boats_before_the_positions() {
    let host = serve(vec![
        (
            "/JSON/rmsr2024/RaceSetup",
            "200 OK",
            fixture("yellowbrick/rmsr2024-RaceSetup.json"),
        ),
        (
            "/BIN/rmsr2024/AllPositions3",
            "200 OK",
            fixture("yellowbrick/rmsr2024-AllPositions3-first3.bin"),
        ),
    ]);
    let client = YellowBrick::at(&host, &host);
    let event = client.resolve("yb.tl/rmsr2024").expect("resolves");
    let fetcher =
        Fetcher::new("YellowBrick", Duration::from_secs(10), Arc::default()).expect("a client");
    let mut listed = Vec::new();
    let out = client
        .fetch_listed(&event, &fetcher, &mut |_| {}, &mut |e| listed.push(e))
        .expect("fetches");
    assert_eq!(listed.len(), 1, "listed once");
    let list = &listed[0];
    assert_eq!(list.title, "Rolex Middle Sea Race 2024");
    assert_eq!(list.boats.len(), 112);
    assert!(list.boats.iter().all(|b| b.fixes.is_empty()));
    let names = |e: &pe_trackers::TrackerEvent| {
        e.boats
            .iter()
            .map(|b| (b.id.clone(), b.name.clone(), b.division.clone()))
            .collect::<Vec<_>>()
    };
    assert_eq!(
        names(list),
        names(&out),
        "the same boats, in the same order"
    );
    assert_eq!(out.boat("1").map(|b| b.fixes.len()), Some(1689));
}

/// Geovoile lists the config's boats before its tracks and reports.
#[test]
fn geovoile_lists_the_boats_before_the_positions() {
    let host = ultim_routes(fixture("geovoile/24hultim2025/viewer.html"));
    let client = geovoile::Geovoile::at(&host);
    let event = client
        .resolve("https://24hultim.geovoile.com/2025/tracker/")
        .expect("resolves");
    let fetcher =
        Fetcher::new("Geovoile", Duration::from_secs(10), Arc::default()).expect("a client");
    let mut listed = Vec::new();
    let out = client
        .fetch_listed(&event, &fetcher, &mut |_| {}, &mut |e| listed.push(e))
        .expect("fetches");
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].title, "24H Ultim");
    assert_eq!(listed[0].event, out.event);
    assert_eq!(listed[0].boats.len(), 14);
    assert!(listed[0].boats.iter().all(|b| b.fixes.is_empty()));
    assert_eq!(listed[0].boats[0].name, out.boats[0].name);
    assert_eq!(out.boat("4").map(|b| b.fixes.len()), Some(470));
}

// --- Finished-only library scraping -----------------------------------------
const SCRAPE_NOW: i64 = 1_800_000_000;
fn scrape_fetcher() -> Fetcher {
    Fetcher::new("scrape test", Duration::from_secs(5), Arc::default()).unwrap()
}

#[test]
fn yellowbrick_scraping_checks_all_results_before_any_position_request() {
    use pe_trackers::library::completion::ScrapeFetch;
    let original: serde_json::Value = serde_json::from_str(
        &fixture("yellowbrick/rmsr2024-RaceSetup.json")
            .iter()
            .map(|b| char::from(*b))
            .collect::<String>(),
    )
    .unwrap();
    for scenario in ["finished", "racing", "unknown", "future"] {
        let mut setup = original.clone();
        if scenario != "finished" {
            setup["teams"][0]
                .as_object_mut()
                .unwrap()
                .remove("finishedAt");
            setup["teams"][0]["status"] = match scenario {
                "racing" => "RACING".into(),
                "unknown" => serde_json::Value::Null,
                _ => "FINISHED".into(),
            };
            if scenario == "future" {
                setup["stop"] = (SCRAPE_NOW + 1).into();
            }
        }
        let (host, seen) = serve_recorded(vec![
            (
                "/JSON/rmsr2024/RaceSetup",
                "200 OK",
                serde_json::to_vec(&setup).unwrap(),
            ),
            (
                "/BIN/rmsr2024/AllPositions3",
                "200 OK",
                fixture("yellowbrick/rmsr2024-AllPositions3-first3.bin"),
            ),
        ]);
        let client = YellowBrick::at(&host, &host);
        let event = client.resolve("rmsr2024").unwrap();
        let result = client
            .fetch_for_scrape(&event, &scrape_fetcher(), &mut |_| {}, SCRAPE_NOW)
            .unwrap();
        assert_eq!(
            matches!(result, ScrapeFetch::Finished(_)),
            scenario == "finished",
            "{scenario}"
        );
        let paths = seen.lock().unwrap();
        assert_eq!(
            paths.iter().any(|p| p.contains("AllPositions3")),
            scenario == "finished",
            "{paths:?}"
        );
        assert!(!paths.iter().any(|p| p.ends_with(".kml")));
    }
}

// Literal-only encoding of synthetic report changes, using Appendix A's
// keystream. The viewer and position response remain recorded provider files.
fn literal_hwx(bytes: &[u8], seeds: geovoile::Seeds) -> Vec<u8> {
    let [mut x, mut y, mut z, mut w] = seeds.0;
    let mut enc = |b: u8| {
        let out = b ^ x as u8;
        let mut t = x ^ ((x << 11) & 0xff_ffff);
        t ^= (t >> 8) & 0xff_ffff;
        x = y;
        y = z;
        z = w;
        w ^= (w >> 19) & 0xff_ffff;
        w ^= t;
        out
    };
    let len = bytes.len();
    let mut out = vec![
        0,
        enc((len >> 16) as u8),
        enc((len >> 8) as u8),
        enc(len as u8),
    ];
    for chunk in bytes.chunks(8) {
        out.push(out.len() as u8 ^ 0xa3);
        out.extend(chunk.iter().map(|b| enc(*b)));
    }
    out
}

#[test]
fn geovoile_scraping_does_not_request_tracks_without_terminal_reports() {
    use pe_trackers::library::completion::ScrapeFetch;
    let page = fixture("geovoile/24hultim2025/viewer.html");
    let seeds = geovoile::parse_viewer(&String::from_utf8_lossy(&page))
        .unwrap()
        .seeds;
    let config = literal_hwx(
        br#"<config name="Test" date="2025-09-27T10:00:00Z"><boat id="4" name="Boat"/></config>"#,
        seeds,
    );
    for scenario in ["finished", "racing", "unknown", "future"] {
        let status = if scenario == "racing" { "RAC" } else { "ARV" };
        let date = if scenario == "future" {
            "2099-01-01T00:00:00Z"
        } else {
            "2025-09-28T10:00:00Z"
        };
        let reports = serde_json::json!({"reports":{"columns":["boat","heading","speed","racestatus"],"history":[{"date":date,"lines":[[4,0,0,status]]}]}});
        let (host, seen) = serve_recorded(vec![
            ("/2025/tracker/", "200 OK", page.clone()),
            (
                "/2025/tracker/resources/versions/v*",
                "200 OK",
                fixture("geovoile/24hultim2025/versions.txt"),
            ),
            (
                "/2025/tracker/resources/config/v20251006074618",
                "200 OK",
                config.clone(),
            ),
            (
                "/2025/tracker/resources/reports/v20250928152939",
                "200 OK",
                if scenario == "unknown" {
                    vec![]
                } else {
                    literal_hwx(&serde_json::to_vec(&reports).unwrap(), seeds)
                },
            ),
            (
                "/2025/tracker/resources/tracks/v20250928152939",
                "200 OK",
                fixture("geovoile/24hultim2025/tracks.hwx"),
            ),
        ]);
        let client = geovoile::Geovoile::at(&host);
        let event = client
            .resolve("https://24hultim.geovoile.com/2025/tracker/")
            .unwrap();
        let result = client
            .fetch_for_scrape(&event, &scrape_fetcher(), &mut |_| {}, SCRAPE_NOW)
            .unwrap();
        assert_eq!(
            matches!(result, ScrapeFetch::Finished(_)),
            scenario == "finished",
            "{scenario}"
        );
        let paths = seen.lock().unwrap();
        assert_eq!(
            paths.iter().any(|p| p.contains("/tracks/")),
            scenario == "finished",
            "{paths:?}"
        );
    }
}

#[test]
fn bluewater_scraping_discards_unfinished_combined_responses() {
    use pe_trackers::library::completion::ScrapeFetch;
    let original: serde_json::Value = serde_json::from_slice(&fixture(
        "bluewater/melbournehobartwestcoaster2025-race.json",
    ))
    .unwrap();
    for scenario in ["finished", "racing", "unknown", "future"] {
        let mut body = original.clone();
        if scenario != "finished" {
            body["race"]["boats"][0]
                .as_object_mut()
                .unwrap()
                .remove("finishTime");
            if scenario == "unknown" {
                body["race"]["boats"][0]["status"] = serde_json::Value::Null;
            }
            if scenario == "future" {
                body["race"]["trackTimeFinish"] = "2099-01-01T00:00:00Z".into();
            }
        }
        let (host, _) = serve_recorded(vec![(
            "/api/race/test",
            "200 OK",
            serde_json::to_vec(&body).unwrap(),
        )]);
        let client = BlueWaterTracks::at(&host);
        let event = client.resolve("test").unwrap();
        let result = client
            .fetch_for_scrape(&event, &scrape_fetcher(), &mut |_| {}, SCRAPE_NOW)
            .unwrap();
        assert_eq!(
            matches!(result, ScrapeFetch::Finished(_)),
            scenario == "finished",
            "{scenario}"
        );
    }
}
