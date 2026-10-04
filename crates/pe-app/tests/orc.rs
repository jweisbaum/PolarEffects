#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The ORC polars section over IPC (spec.md 5), driven through the functions
//! the Tauri commands call.

mod common;

use common::TempRoot;
use pe_app::error::AppError;
use pe_app::orc::{self, OrcFilters};
use pe_app::{edit, projects};
use pe_core::SourceKind;
use pe_core::source::PALETTE;

fn eratosthenes(app: &pe_app::commands::AppState) -> orc::OrcHit {
    let found = orc::search(app, "GBR/1124", OrcFilters::default(), 10).unwrap();
    let hit = found.hits[0].clone();
    assert_eq!(hit.name, "Eratosthenes");
    hit
}

#[test]
fn the_catalogue_info_names_its_source() {
    let root = TempRoot::new("orc-info");
    let info = orc::info(&root.state()).unwrap();
    // Nothing was scraped into this session's folder.
    assert_eq!(info.scraped, 0);
    assert_eq!(info.scraped_at, None);
    assert!(info.records > 18_000);
    assert_eq!(info.source, "jieter/orc-data");
    assert_eq!(info.commit.len(), 40);
    assert!(info.countries.contains(&"GBR".to_owned()));
    assert!(info.year_min.unwrap() < info.year_max.unwrap());
}

#[test]
fn search_works_without_a_project_and_filters_apply() {
    let root = TempRoot::new("orc-search");
    let app = root.state();
    let hit = eratosthenes(&app);
    assert_eq!(hit.sail_no, "GBR 1124");
    assert_eq!(hit.model.as_deref(), Some("Swan 112"));
    assert_eq!(hit.builder.as_deref(), Some("Nautor"));
    assert_eq!(hit.year, Some(1999));
    assert!(!hit.in_project);
    assert_eq!(hit.thumb.len(), 3);

    let found = orc::search(
        &app,
        "swan",
        OrcFilters {
            year_min: Some(1999),
            year_max: Some(1999),
            country: Some("GBR".to_owned()),
            ..OrcFilters::default()
        },
        200,
    )
    .unwrap();
    assert!(found.total >= 1);
    assert!(
        found
            .hits
            .iter()
            .all(|h| h.year == Some(1999) && h.country == "GBR")
    );
    assert!(found.hits.iter().any(|h| h.name == "Eratosthenes"));

    let nothing = orc::search(&app, "  ", OrcFilters::default(), 50).unwrap();
    assert_eq!((nothing.total, nothing.hits.len()), (0, 0));
}

#[test]
fn field_queries_arrive_as_the_interface_sends_them() {
    let root = TempRoot::new("orc-fields");
    let app = root.state();
    // The JSON `api.orcSearch` sends, typed into two fields.
    let filters: OrcFilters = serde_json::from_value(serde_json::json!({
        "year_min": null, "year_max": null, "country": null,
        "name": "", "sail_no": "gbr/1124", "model": "SWAN", "builder": "",
        "designer": "", "certificate_year": "",
    }))
    .unwrap();
    let found = orc::search(&app, "", filters.clone(), 50).unwrap();
    assert_eq!(found.hits[0].name, "Eratosthenes");
    assert!(
        found
            .hits
            .iter()
            .all(|h| h.model.as_deref().unwrap_or("").contains("Swan"))
    );
    // A field is searched on its own: Swan is a model, not a sail number.
    let wrong_field = OrcFilters {
        sail_no: "swan".to_owned(),
        ..OrcFilters::default()
    };
    assert_eq!(orc::search(&app, "", wrong_field, 50).unwrap().total, 0);
    // Older callers that send no field queries still work.
    let bare: OrcFilters = serde_json::from_value(
        serde_json::json!({"year_min": 1999, "year_max": 1999, "country": "GBR"}),
    )
    .unwrap();
    assert_eq!(bare.name, "");
}

#[test]
fn add_copies_the_record_with_the_next_colour_as_one_undo_and_asks_before_a_duplicate() {
    let root = TempRoot::new("orc-add");
    let app = root.state();
    assert!(matches!(
        orc::add(&app, 0, false),
        Err(AppError::NoProjectOpen)
    ));
    projects::create(&app, "ORC".to_owned(), None, false).unwrap();
    let hit = eratosthenes(&app);

    let project = orc::add(&app, hit.id, false).unwrap();
    assert!(project.dirty);
    let source = &project.sources[0];
    assert_eq!(source.label, "Eratosthenes");
    assert_eq!(source.kind, "orc");
    assert_eq!(source.colour, PALETTE[0]);
    let summary = source.orc.as_ref().unwrap();
    assert_eq!(summary.sail_no, "GBR 1124");
    assert_eq!(summary.model.as_deref(), Some("Swan 112"));
    // 8 angles × 7 wind speeds, plus a beat and a run point per wind speed.
    assert_eq!(source.count, 56 + 14);

    // It is now marked, and adding it again is refused until allowed.
    assert!(eratosthenes(&app).in_project);
    let again = orc::add(&app, hit.id, false).unwrap_err();
    assert!(matches!(again, AppError::DuplicateCertificate { .. }));
    assert_eq!(
        serde_json::to_value(&again).unwrap()["kind"],
        "orc-duplicate"
    );
    let twice = orc::add(&app, hit.id, true).unwrap();
    assert_eq!(twice.sources.len(), 2);
    assert_eq!(twice.sources[1].colour, PALETTE[1]);

    // Each add is one undo entry.
    assert_eq!(edit::undo_last(&app).unwrap().sources.len(), 1);
    assert_eq!(edit::undo_last(&app).unwrap().sources.len(), 0);
    assert_eq!(edit::redo_next(&app).unwrap().sources.len(), 1);

    // Remove is undoable.
    let id = projects::summary(&app).unwrap().unwrap().sources[0].id;
    assert!(edit::source_remove(&app, id).unwrap().sources.is_empty());
    assert!(!eratosthenes(&app).in_project);
    assert_eq!(
        edit::undo_last(&app).unwrap().sources[0].label,
        "Eratosthenes"
    );

    assert!(matches!(
        orc::add(&app, u32::MAX, false),
        Err(AppError::BadOption { .. })
    ));
}

/// Acceptance (plan.md M5): a known certificate's VPP, once added and
/// converted to a polar, matches the certificate to 0.01 kn. The expected
/// numbers are copied by hand from orc-data's `site/data/GBR/1124.json`
/// (Eratosthenes, Swan 112); the beat and run speeds are the VMG over the
/// cosine of the angle, worked by hand from the same file.
#[test]
fn a_known_certificate_becomes_the_polar_it_publishes() {
    let root = TempRoot::new("orc-polar");
    let app = root.state();
    projects::create(&app, "ORC".to_owned(), None, false).unwrap();
    orc::add(&app, eratosthenes(&app).id, false).unwrap();
    let record = app
        .with_session(|session| {
            let open = session.require_open()?;
            match &open.project.sources[0].kind {
                SourceKind::Orc { record } => Ok(record.as_ref().clone()),
                _ => panic!("not an ORC source"),
            }
        })
        .unwrap();
    let polar = pe_polar::vpp_to_polar(&record.vpp);
    polar.validate().unwrap();
    assert_eq!(polar.tws, vec![6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0]);

    let at = |twa: f64, tws: f64| {
        let i = polar.twa.iter().position(|a| *a == twa).unwrap();
        let j = polar.tws.iter().position(|s| *s == tws).unwrap();
        polar.bsp[i][j]
    };
    let expect = [
        (52.0, 6.0, 6.87),
        (52.0, 20.0, 11.53),
        (75.0, 12.0, 11.64),
        (110.0, 16.0, 12.84),
        (150.0, 8.0, 6.77),
        (150.0, 20.0, 12.77),
        // Beat: 4.33 kn VMG at 47.1°, 7.72 kn at 41.7°.
        (47.1, 6.0, 6.3609),
        (41.7, 20.0, 10.3397),
        // Run: 4.48 kn VMG at 141.2°, 11.06 kn at 150.5°.
        (141.2, 6.0, 5.7485),
        (150.5, 20.0, 12.7074),
    ];
    for (twa, tws, bsp) in expect {
        let got = at(twa, tws).unwrap_or_else(|| panic!("no speed at {twa}°, {tws} kn"));
        assert!(
            (got - bsp).abs() <= 0.01,
            "{twa}° {tws} kn: {got} is not {bsp}"
        );
    }
    // Nothing invented: the beat angle at 6 kn has no speed at 20 kn.
    assert_eq!(at(47.1, 20.0), None);
    assert_eq!(polar.twa.first(), Some(&41.7));
    assert_eq!(polar.twa.last(), Some(&150.5));
}
