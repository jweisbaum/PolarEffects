#![allow(clippy::expect_used, clippy::unwrap_used, reason = "integration tests")]
//! Catalogue refreshes never duplicate entries or rewrite project sources.
mod common;
use common::TempRoot;
use pe_app::{edit, orc::OrcFilters, orr, projects};
use pe_core::{SourceKind, orr::OrrRecord};

fn record() -> OrrRecord {
    let list = pe_trackers::orr::parse_list(
        include_str!("../../pe-trackers/tests/fixtures/orr/list.html"),
        2026,
    )
    .unwrap();
    pe_trackers::orr::parse_certificate(
        include_str!("../../pe-trackers/tests/fixtures/orr/phoenix.html"),
        &list[0],
    )
    .unwrap()
    .remove(0)
}

#[test]
fn refresh_deduplicates_and_imported_sources_round_trip_without_mutation() {
    let root = TempRoot::new("orr-refresh");
    let app = root.state();
    let mut original = record();
    assert_eq!(original.details.as_ref().unwrap().ratings.len(), 5);
    original.sku = "fixture-orr-unique".into();
    let key = original.key();
    assert_eq!(
        orr::merge_records(&app, vec![original.clone(), original.clone()]).unwrap(),
        (1, 0)
    );
    assert_eq!(
        orr::merge_records(&app, vec![original.clone()]).unwrap(),
        (0, 0)
    );
    projects::create(&app, "ORR fixture".into(), None, false).unwrap();
    orr::add(&app, &key).unwrap();
    orr::add(&app, &key).unwrap();
    assert_eq!(
        app.with_session(|s| Ok(s.require_open()?.project.sources.len()))
            .unwrap(),
        1
    );
    // A repeated Add must not create another undo entry.
    edit::undo_last(&app).unwrap();
    assert_eq!(
        app.with_session(|s| Ok(s.require_open()?.project.sources.len()))
            .unwrap(),
        0
    );
    edit::redo_next(&app).unwrap();
    let mut update = original.clone();
    update.name = "Updated catalogue only".into();
    update.polar.bsp[0][0] = Some(4.25);
    update
        .details
        .as_mut()
        .unwrap()
        .ratings
        .get_mut("gph_ratings")
        .unwrap()[0]
        .spin = Some("600.0".into());
    assert_eq!(
        orr::merge_records(&app, vec![update.clone(), update]).unwrap(),
        (0, 1)
    );
    app.with_session(|s| {
        assert!(matches!(&s.require_open()?.project.sources[0].kind, SourceKind::Orr { record } if **record == original));
        Ok(())
    }).unwrap();
    projects::save_as(&app, root.file("source.wpsproj")).unwrap();
    let bytes = std::fs::read(root.file("source.wpsproj")).unwrap();
    projects::open(&app, root.file("source.wpsproj"), true).unwrap();
    app.with_session(|s| {
        assert!(matches!(&s.require_open()?.project.sources[0].kind, SourceKind::Orr { record } if **record == original));
        Ok(())
    }).unwrap();
    projects::save(&app).unwrap();
    assert_eq!(std::fs::read(root.file("source.wpsproj")).unwrap(), bytes);
    let reopened = root.state();
    assert_eq!(
        orr::records(&reopened)
            .unwrap()
            .iter()
            .filter(|r| r.key() == key)
            .count(),
        1
    );
    assert_eq!(
        orr::records(&reopened)
            .unwrap()
            .iter()
            .find(|r| r.key() == key)
            .unwrap()
            .name,
        "Updated catalogue only"
    );
}

#[test]
fn legacy_records_load_and_cannot_mask_complete_bundled_certificates() {
    let root = TempRoot::new("orr-legacy");
    let mut encoded = serde_json::to_value(record()).unwrap();
    encoded.as_object_mut().unwrap().remove("details");
    let legacy: OrrRecord = serde_json::from_value(encoded).unwrap();
    assert!(legacy.details.is_none());
    let mut project = pe_core::Project::new("Legacy ORR", pe_core::project::Boat::default(), 0);
    let id = project.allocate_source_id();
    project.sources.push(pe_core::Source::new(
        id,
        "Legacy",
        pe_core::Colour::parse("#4e79a7").unwrap(),
        SourceKind::Orr {
            record: Box::new(legacy.clone()),
        },
    ));
    project.schema_version = 3;
    let upgraded =
        pe_core::io::from_json(&pe_core::io::to_canonical_json(&project).unwrap()).unwrap();
    assert_eq!(upgraded.schema_version, pe_core::project::SCHEMA_VERSION);
    assert!(
        matches!(&upgraded.sources[0].kind, SourceKind::Orr { record } if record.details.is_none())
    );
    let app = root.state();
    pe_core::io::write_atomic(
        &app.paths.config_dir.join("orr-catalogue.json"),
        &serde_json::to_vec(&vec![legacy.clone()]).unwrap(),
    )
    .unwrap();
    let catalogue = orr::records(&app).unwrap();
    assert!(
        catalogue
            .iter()
            .find(|r| r.key() == legacy.key())
            .unwrap()
            .details
            .is_some()
    );
}

#[test]
fn every_bundled_variant_carries_the_complete_certificate_payload() {
    let root = TempRoot::new("orr-bundle-completeness");
    let app = root.state();
    let records = orr::records(&app).unwrap();
    assert_eq!(records.len(), 600);
    let mut keys = std::collections::BTreeSet::new();
    let mut certificates = std::collections::BTreeMap::new();
    for record in records.iter() {
        assert!(keys.insert(record.key()));
        let details = record.details.as_ref().unwrap();
        assert!(details.fields.len() >= 250, "{}", record.sku);
        assert_eq!(details.list_fields.len(), 21, "{}", record.sku);
        assert_eq!(details.tables.len(), 4, "{}", record.sku);
        assert_eq!(details.ratings.len(), 5, "{}", record.sku);
        assert!(details.ratings.values().map(Vec::len).sum::<usize>() >= 61);
        if let Some(previous) = certificates.insert(&record.sku, details) {
            assert_eq!(previous, details);
        }
    }
    assert_eq!(certificates.len(), 300);
}

#[test]
fn full_certificate_build_year_and_builder_are_searchable() {
    let root = TempRoot::new("orr-complete-search");
    let app = root.state();
    let mut fixture = record();
    fixture.sku = "complete-certificate-fixture".into();
    fixture.name = "Complete fixture".into();
    orr::merge_records(&app, vec![fixture]).unwrap();
    let mut filters = OrcFilters {
        year_min: Some(1994),
        year_max: Some(1994),
        ..Default::default()
    };
    filters.builder = "TPI".into();
    let found = orr::search(&app, "Complete fixture", filters.clone(), 50, 0).unwrap();
    assert_eq!(found.total, 1);
    assert_eq!(found.hits[0].builder.as_deref(), Some("TPI"));
    assert_eq!(found.hits[0].year, Some(1994));
    filters.year_min = Some(1995);
    filters.year_max = None;
    assert_eq!(
        orr::search(&app, "Complete fixture", filters, 50, 0)
            .unwrap()
            .total,
        0
    );
}

#[test]
fn measured_search_has_stable_nonoverlapping_pages_and_inclusive_bounds() {
    let root = TempRoot::new("orr-search");
    let app = root.state();
    let mut base = record();
    base.name = "Measurement fixture".into();
    base.size.loa = Some(12.244);
    base.size.displacement_kg = Some(6840.0);
    let mut rows = Vec::new();
    for i in 0..5 {
        let mut r = base.clone();
        r.sku = format!("fixture-{i}");
        rows.push(r);
    }
    let mut missing = base.clone();
    missing.sku = "fixture-missing".into();
    missing.size.loa = None;
    rows.push(missing);
    orr::merge_records(&app, rows).unwrap();
    let mut filters = OrcFilters {
        size_min: vec![Some(12.244)],
        size_max: vec![Some(12.244)],
        ..Default::default()
    };
    let first = orr::search(&app, "measurement fixture", filters.clone(), 3, 0).unwrap();
    let second = orr::search(&app, "measurement fixture", filters.clone(), 3, 3).unwrap();
    assert_eq!(first.total, 5);
    assert_eq!(second.hits.len(), 2);
    assert!(
        first
            .hits
            .iter()
            .all(|a| second.hits.iter().all(|b| a.id != b.id))
    );
    assert!(
        orr::search(&app, "measurement fixture", filters.clone(), 3, 6)
            .unwrap()
            .hits
            .is_empty()
    );
    filters.size_min = vec![Some(12.245)];
    filters.size_max = vec![];
    assert_eq!(
        orr::search(&app, "measurement fixture", filters.clone(), 3, 0)
            .unwrap()
            .total,
        0
    );
    filters.size_max = vec![Some(10.0)];
    assert!(orr::search(&app, "", filters, 3, 0).is_err());
}
