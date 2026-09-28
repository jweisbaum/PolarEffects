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

use std::path::PathBuf;

use pe_trackers::geovoile::{self, Seeds};
use pe_trackers::yellowbrick;

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
