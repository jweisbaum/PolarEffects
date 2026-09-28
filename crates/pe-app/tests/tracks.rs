#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! File tracks end to end (spec.md 7.1, 7.3, 7.4, 7.6, 9.1, 10.3): inspect,
//! map columns, pick boats, import as one undo entry, edit filters and
//! derivation undoably, exclude samples, save and reload, and the map and
//! 3D packets — all through the functions the Tauri commands call.

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::map_tracks::{self, FLAG_EXCLUDED, FLAG_FILTERED};
use pe_app::polar3d::{self, scene_of};
use pe_app::tracks::{self, CsvMappingInput, TrackFileRequest, TrackFilters};
use pe_app::{edit, projects};
use pe_core::SampleId;

const NOON: i64 = 1_753_531_200;

/// Two boats in one GeoJSON file, Alpha crossing the antimeridian eastward
/// at 0.01° per 10 minutes, Bravo stationary.
fn two_boats() -> String {
    let mut features = Vec::new();
    for (k, lon) in [179.99, -180.0, -179.99].iter().enumerate() {
        features.push(format!(
            r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{lon},0]}},"properties":{{"time":{},"boat":"Alpha"}}}}"#,
            NOON + 600 * k as i64
        ));
    }
    for k in 0..3 {
        features.push(format!(
            r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[-1.3,50.1]}},"properties":{{"time":{},"boat":"Bravo"}}}}"#,
            NOON + 600 * k
        ));
    }
    format!(
        r#"{{"type":"FeatureCollection","features":[{}]}}"#,
        features.join(",")
    )
}

fn project(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Tracks".to_owned(), None, false).unwrap();
    app
}

fn write(root: &TempRoot, name: &str, text: &str) -> String {
    let path = root.file(name);
    std::fs::write(&path, text).unwrap();
    path
}

fn request(path: &str) -> TrackFileRequest {
    TrackFileRequest {
        path: path.to_owned(),
        mapping: None,
        boats: None,
    }
}

#[test]
fn a_multi_boat_geojson_offers_its_boats_and_imports_the_chosen_ones() {
    let root = TempRoot::new("tracks-geojson");
    let app = project(&root);
    let path = write(&root, "race.geojson", &two_boats());

    let inspected = tracks::inspect(&path, None);
    assert_eq!(inspected.kind, "geojson");
    assert!(inspected.failure.is_none());
    let boats: Vec<(&str, u32)> = inspected
        .boats
        .iter()
        .map(|b| (b.name.as_str(), b.fixes))
        .collect();
    assert_eq!(boats, [("Alpha", 3), ("Bravo", 3)]);
    assert_eq!(inspected.boats[0].start, NOON);
    assert_eq!(inspected.boats[0].end, NOON + 1200);

    let result = tracks::import(
        &app,
        &[TrackFileRequest {
            boats: Some(vec!["Alpha".to_owned()]),
            ..request(&path)
        }],
    )
    .unwrap();
    assert!(result.failures.is_empty());
    assert_eq!(result.imported.len(), 1);
    assert_eq!(result.imported[0].label, "Alpha");
    assert_eq!(result.imported[0].heading_derived, 3);
    let source = &result.project.sources[0];
    assert_eq!(source.kind, "track");
    let track = source.track.as_ref().unwrap();
    assert_eq!(track.boat_name.as_deref(), Some("Alpha"));
    assert_eq!(track.event_title, "race.geojson");
    assert_eq!((track.start, track.end), (Some(NOON), Some(NOON + 1200)));
    assert_eq!(track.samples, 3);
    assert_eq!(track.with_wind, 0);
    assert_eq!(track.env_status, "not_fetched");
    assert_eq!(track.max_gap_s, 3 * 3600);
    assert_eq!(result.project.undo_label.as_deref(), Some("Import track"));

    // One undo takes it back out.
    let undone = edit::undo_last(&app).unwrap();
    assert!(undone.sources.is_empty());
}

#[test]
fn a_csv_is_mapped_guessed_then_corrected_and_its_disorder_reported() {
    let root = TempRoot::new("tracks-csv");
    let app = project(&root);
    let path = write(
        &root,
        "log.csv",
        "when;Latitude;Longitude;Speed (km/h)\n\
         26/07/2025 12:10;50,0;-1,0;18,52\n\
         26/07/2025 12:00;50,0;-1,0;18,52\n\
         26/07/2025 12:00;50,0;-1,0;18,52\n\
         26/07/2025 12:20;50,0;-1,0;\n",
    );
    let inspected = tracks::inspect(&path, None);
    let csv = inspected.csv.as_ref().unwrap();
    assert_eq!(
        csv.header,
        ["when", "Latitude", "Longitude", "Speed (km/h)"]
    );
    assert_eq!(csv.rows.len(), 4);
    // "when" is no name the guess knows, and the dates are no format it can
    // tell: the dialog must ask.
    assert_eq!(csv.mapping.time, None);
    assert_eq!(
        (csv.mapping.lat, csv.mapping.lon, csv.mapping.speed),
        (Some(1), Some(2), Some(3))
    );
    assert_eq!(csv.mapping.speed_unit, "kmh");
    assert_eq!(inspected.failure.as_ref().unwrap().reason, "no-mapping");

    let mapping = CsvMappingInput {
        time: Some(0),
        time_format: "custom".to_owned(),
        custom_format: "%d/%m/%Y %H:%M".to_owned(),
        ..csv.mapping.clone()
    };
    let again = tracks::inspect(&path, Some(&mapping));
    assert!(again.failure.is_none(), "{:?}", again.failure);
    assert_eq!(again.boats.len(), 1);
    assert_eq!(again.boats[0].fixes, 4);

    let result = tracks::import(
        &app,
        &[TrackFileRequest {
            mapping: Some(mapping),
            ..request(&path)
        }],
    )
    .unwrap();
    let line = &result.imported[0];
    assert_eq!(line.label, "log");
    assert_eq!((line.fixes, line.out_of_order, line.duplicates), (3, 1, 1));
    // 18.52 km/h is exactly 10 kn; the last row gave no speed.
    assert_eq!((line.speed_given, line.speed_derived), (2, 1));
    app.with_session(|session| {
        let open = session.require_open()?;
        let track = open.project.sources[0].track().unwrap();
        assert!((track.fixes[0].sog.unwrap() - 10.0).abs() < 1e-12);
        assert_eq!(track.fixes[0].t, NOON);
        Ok(())
    })
    .unwrap();
}

#[test]
fn bad_files_are_reported_with_where_and_the_rest_still_import() {
    let root = TempRoot::new("tracks-bad");
    let app = project(&root);
    let good = write(&root, "good.geojson", &two_boats());
    let bad = write(
        &root,
        "bad.csv",
        "time,lat,lon\n1753531200,50,-1\nnoon,50,-1\n",
    );
    let missing = root.file("missing.csv");
    let result = tracks::import(&app, &[request(&bad), request(&good), request(&missing)]).unwrap();
    assert_eq!(result.imported.len(), 2);
    assert_eq!(result.project.undo_label.as_deref(), Some("Import tracks"));
    assert_eq!(result.failures.len(), 2);
    let csv = &result.failures[0];
    assert_eq!(
        (csv.file.as_str(), csv.line, csv.column, csv.reason.as_str()),
        ("bad.csv", Some(3), Some(1), "bad-time")
    );
    assert_eq!(result.failures[1].reason, "unreadable");
}

fn imported(root: &TempRoot) -> (AppState, u64, Vec<u64>) {
    let app = project(root);
    let path = write(root, "race.geojson", &two_boats());
    let result = tracks::import(&app, &[request(&path)]).unwrap();
    let id = result.project.sources[0].id;
    let ids = app
        .with_session(|session| {
            let open = session.require_open()?;
            Ok(open.project.sources[0]
                .track()
                .unwrap()
                .samples
                .iter()
                .map(|s| s.id.raw())
                .collect())
        })
        .unwrap();
    (app, id, ids)
}

#[test]
fn filters_and_derivation_are_edited_undoably() {
    let root = TempRoot::new("tracks-edit");
    let (app, id, _) = imported(&root);
    let before = projects::summary(&app).unwrap().unwrap();
    let alpha = before.sources[0].track.clone().unwrap();
    // Alpha sails at 3.6 kn, over the 1 kn default minimum.
    assert_eq!(alpha.filtered, 0);
    assert_eq!(alpha.filters.min_bsp, Some(1.0));
    assert_eq!(alpha.filters.max_heading_change, Some(30.0));
    // Bravo is stationary: under the minimum boat speed.
    assert_eq!(before.sources[1].track.as_ref().unwrap().filtered, 3);
    assert_eq!(before.sources[1].used, Some(0));

    let window = TrackFilters {
        time_start: Some(NOON + 300),
        min_bsp: Some(4.0),
        ..alpha.filters.clone()
    };
    let after = tracks::track_filters_set(&app, id, &window).unwrap();
    assert_eq!(after.undo_label.as_deref(), Some("Change sample filters"));
    assert_eq!(after.sources[0].track.as_ref().unwrap().filtered, 3);
    assert_eq!(after.sources[0].used, Some(0));
    let undone = edit::undo_last(&app).unwrap();
    assert_eq!(
        undone.sources[0].track.as_ref().unwrap().filters,
        alpha.filters
    );

    let upside_down = TrackFilters {
        min_bsp: Some(5.0),
        max_bsp: Some(2.0),
        ..alpha.filters.clone()
    };
    assert!(tracks::track_filters_set(&app, id, &upside_down).is_err());

    // A 5-minute maximum gap leaves every fix of a 10-minute track alone.
    let rederived = tracks::track_derivation_set(&app, id, 300, "given").unwrap();
    assert_eq!(
        rederived.undo_label.as_deref(),
        Some("Change heading and speed derivation")
    );
    let motion = |app: &AppState| {
        app.with_session(|session| {
            let open = session.require_open()?;
            Ok(open.project.sources[0]
                .track()
                .unwrap()
                .samples
                .iter()
                .map(|s| (s.heading, s.speed))
                .collect::<Vec<_>>())
        })
        .unwrap()
    };
    assert!(motion(&app).iter().all(|m| *m == (None, None)));
    edit::undo_last(&app).unwrap();
    assert!(
        motion(&app)
            .iter()
            .all(|m| m.0.is_some_and(|h| (h - 90.0).abs() < 1e-9))
    );
    assert!(tracks::track_derivation_set(&app, id, 0, "given").is_err());
    assert!(tracks::track_derivation_set(&app, id, 600, "sometimes").is_err());
}

#[test]
fn samples_are_excluded_and_included_through_the_3d_command() {
    let root = TempRoot::new("tracks-exclude");
    let (app, id, ids) = imported(&root);
    let summary = polar3d::excluded_set(&app, &[], &[ids[2], ids[0]], true).unwrap();
    assert_eq!(summary.undo_label.as_deref(), Some("Exclude samples"));
    assert_eq!(summary.sources[0].track.as_ref().unwrap().excluded, 2);
    assert_eq!(summary.sources[0].used, Some(1));
    // Already excluded: nothing to do, nothing recorded.
    let same = polar3d::excluded_set(&app, &[], &[ids[0]], true).unwrap();
    assert_eq!(same.revision, summary.revision);
    let included = polar3d::excluded_set(&app, &[], &[ids[0], ids[1]], false).unwrap();
    assert_eq!(included.undo_label.as_deref(), Some("Include samples"));
    assert_eq!(included.sources[0].track.as_ref().unwrap().excluded, 1);
    assert!(polar3d::excluded_set(&app, &[], &[999_999], true).is_err());

    // The map shows the exclusion; hover details say so too.
    let bytes = map_tracks::tracks_bytes(&app).unwrap();
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let fixes = word(3) as usize;
    let flags_at = 4 + 2 * 5 + fixes * 2 + fixes * 2;
    assert_eq!(word(flags_at + 2), FLAG_EXCLUDED);
    // Bravo, stationary, is filtered out (under 1 kn).
    assert_eq!(word(flags_at + 3), FLAG_FILTERED);
    let details = tracks::details(&app, id, ids[2]).unwrap();
    assert!(details.excluded && !details.filtered);
    assert_eq!(details.t, NOON + 1200);
    assert_eq!(details.heading_origin.as_deref(), Some("derived"));
    assert!((details.speed.unwrap() - 3.602_556_9).abs() < 1e-6);
}

#[test]
fn the_map_packet_keeps_an_antimeridian_track_continuous() {
    let root = TempRoot::new("tracks-map");
    let (app, _, ids) = imported(&root);
    let bytes = map_tracks::tracks_bytes(&app).unwrap();
    let word = |i: usize| u32::from_le_bytes(bytes[i * 4..i * 4 + 4].try_into().unwrap());
    let float = |i: usize| f32::from_bits(word(i));
    assert_eq!((word(2), word(3)), (2, 6));
    // Alpha: 179.99, then 180 and 180.01 rather than -180 and -179.99.
    let lons: Vec<f32> = (0..3).map(|k| float(14 + 2 * k)).collect();
    assert_eq!(lons, [179.99, 180.0, 180.01]);
    assert_eq!(word(14 + 12), ids[0] as u32);
}

/// A sample without wind has no place in the polar and is not drawn; with
/// environment values on it (put there directly, as M9's fetch will), it is
/// a dot in the 3D scene with its Hs, current, time and flags.
#[test]
fn samples_appear_in_the_3d_scene_once_they_have_wind() {
    let root = TempRoot::new("tracks-scene");
    let (app, _, ids) = imported(&root);
    let scene = app
        .with_session(|session| Ok(scene_of(&session.require_open()?.project)))
        .unwrap();
    assert!(scene.samples.is_empty());

    app.with_session(|session| {
        let open = session.require_open()?;
        let track = open.project.sources[0].track_mut().unwrap();
        for (k, sample) in track.samples.iter_mut().enumerate() {
            sample.tws = Some(12.0);
            sample.twa = Some(45.0 + 10.0 * k as f64);
            sample.hs_m = Some(1.5);
        }
        open.project.sources[0].overlay.excluded_samples = vec![SampleId(ids[1])];
        Ok(())
    })
    .unwrap();
    let scene = app
        .with_session(|session| Ok(scene_of(&session.require_open()?.project)))
        .unwrap();
    assert_eq!(scene.samples.len(), 3);
    assert_eq!(scene.time_origin, NOON);
    let s = &scene.samples[1];
    assert_eq!((s.twa, s.tws, s.hs, s.time), (55.0, 12.0, 1.5, 600.0));
    assert!((s.bsp - 3.602_557).abs() < 1e-4);
    assert!(s.current.is_nan());
    assert!(s.excluded && !s.filtered);
    assert_eq!(s.id, ids[1]);
}

#[test]
fn a_track_project_saves_and_reopens_byte_identically() {
    let root = TempRoot::new("tracks-save");
    let (app, _, ids) = imported(&root);
    polar3d::excluded_set(&app, &[], &[ids[1]], true).unwrap();
    let path = root.file("race.wpsproj");
    projects::save_as(&app, path.clone()).unwrap();
    let first = std::fs::read(&path).unwrap();
    projects::open(&app, path.clone(), true).unwrap();
    projects::save(&app).unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), first);
    let reopened = projects::summary(&app).unwrap().unwrap();
    assert_eq!(reopened.sources[0].track.as_ref().unwrap().excluded, 1);
}

/// The spec.md 13 scale for the map: 50 tracks of 10,000 fixes. What every
/// edit rebuilds (the summary, the map packet, the 3D scene, the plot) and
/// a 10,000-sample exclusion, timed; printed with `--nocapture`.
#[test]
fn fifty_tracks_of_ten_thousand_fixes_stay_interactive() {
    use pe_core::track::{DerivationSettings, Fix, TrackOrigin};
    use pe_core::{Command, Source, SourceKind};

    let root = TempRoot::new("tracks-scale");
    let app = project(&root);
    app.with_session(|session| {
        let open = session.require_open()?;
        for b in 0..50 {
            let project = &mut open.project;
            let fixes: Vec<Fix> = (0..10_000)
                .map(|k| Fix {
                    t: NOON + 60 * k,
                    lat: 45.0 + f64::from(b) * 0.1 + (k as f64 * 0.01).sin() * 0.05,
                    lon: -170.0 + k as f64 * 0.002,
                    cog: None,
                    sog: None,
                })
                .collect();
            let id = project.allocate_source_id();
            let track_id = project.allocate_track_id();
            let (mut track, _) = pe_tracks::build_track(
                track_id,
                TrackOrigin::File {
                    name: format!("{b}.csv"),
                    boat_name: None,
                },
                fixes,
                DerivationSettings::default(),
                || SampleId(0),
            );
            for (k, sample) in track.samples.iter_mut().enumerate() {
                sample.id = project.allocate_sample_id();
                sample.tws = Some(8.0 + (k % 12) as f64);
                sample.twa = Some(30.0 + (k % 150) as f64);
            }
            let colour = project.next_palette_colour();
            let index = project.sources.len();
            let source = Source::new(
                id,
                format!("Boat {b}"),
                colour,
                SourceKind::Track {
                    track: Box::new(track),
                },
            );
            open.apply(Command::AddSource {
                index,
                source: Box::new(source),
            })?;
        }
        Ok(())
    })
    .unwrap();

    let time = |what: &str, f: &mut dyn FnMut()| {
        let started = std::time::Instant::now();
        f();
        let elapsed = started.elapsed();
        println!("{what}: {elapsed:?}");
        elapsed
    };
    let summary = time("project summary", &mut || {
        projects::summary(&app).unwrap();
    });
    let mut bytes = 0;
    time("map packet", &mut || {
        bytes = map_tracks::tracks_bytes(&app).unwrap().len()
    });
    assert_eq!(bytes, 16 + 50 * 20 + 500_000 * 20);
    time("3D scene", &mut || {
        polar3d::scene_bytes(&app).unwrap();
    });
    time("2D plot", &mut || {
        pe_app::polar_plot::plot(&app, Some(12.0)).unwrap();
    });
    let ids: Vec<u64> = app
        .with_session(|session| {
            let open = session.require_open()?;
            Ok(open.project.sources[7]
                .track()
                .unwrap()
                .samples
                .iter()
                .map(|s| s.id.raw())
                .collect())
        })
        .unwrap();
    time("exclude 10,000 samples", &mut || {
        polar3d::excluded_set(&app, &[], &ids, true).unwrap();
    });
    // Generous for an unoptimised debug build of the app crate on a busy
    // machine; the numbers themselves go in the report.
    assert!(summary.as_millis() < 2000, "{summary:?}");
}
