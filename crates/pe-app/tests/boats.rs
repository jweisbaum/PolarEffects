#![allow(clippy::unwrap_used, clippy::expect_used, reason = "integration tests")]
mod common;
use common::TempRoot;
use pe_app::{autosave, boats, edit, projects, tracks};
use pe_core::{io, track::Tracker};
use pe_trackers::{
    TrackerBoat, TrackerEvent,
    event::{EventRef, PositionsFrom},
};
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

#[test]
fn independent_boats_share_a_file_but_not_sources_history_or_ids() {
    let root = TempRoot::new("boat-isolation");
    let state = root.state();
    let first = projects::create(&state, "Fleet".into(), None, false).unwrap();
    let second = boats::add(&state, "Bravo".into()).unwrap();
    let scoped = state.scoped(Some(second.id));
    let file = root.file("wind.csv");
    std::fs::write(&file, "time,lat,lon,cog,sog,tws,twd\n0,0,0,0,6,10,90\n60,0,0.01,0,6,10,90\n120,0,0.02,0,6,12,90\n").unwrap();
    let req = tracks::TrackFileRequest {
        path: file,
        mapping: None,
        boats: None,
    };
    tracks::import(&state, std::slice::from_ref(&req)).unwrap();
    tracks::import(&scoped, &[req]).unwrap();
    let a = projects::summary(&state).unwrap().unwrap();
    let b = projects::summary(&scoped).unwrap().unwrap();
    assert_eq!(
        a.sources[0].id, b.sources[0].id,
        "ids are deliberately local to each boat"
    );
    boats::rename(&scoped, "Charlie".into()).unwrap();
    assert_ne!(
        projects::summary(&state).unwrap().unwrap().boat_name,
        "Charlie"
    );
    edit::undo_last(&scoped).unwrap();
    assert_eq!(
        projects::summary(&scoped).unwrap().unwrap().boat_name,
        "Bravo"
    );
    boats::rename(&scoped, "Charlie".into()).unwrap();
    let path = root.file("fleet.wpsproj");
    projects::save_as(&state, path.clone()).unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let doc = io::from_bytes(&bytes).unwrap();
    assert_eq!(doc.boat_tabs.len(), 1);
    assert_eq!(doc.boat_tabs[0].boat.name, "Charlie");
    assert_eq!(doc.boat_tabs[0].sources[0].track().unwrap().fixes.len(), 3);
    assert_eq!(
        io::to_bytes(&doc).unwrap(),
        bytes,
        "canonical complete-document round trip"
    );
    projects::open(&state, path, true).unwrap();
    assert_eq!(
        boats::list(&state)
            .unwrap()
            .tabs
            .iter()
            .map(|b| b.id)
            .collect::<Vec<_>>(),
        [first.id, second.id]
    );
    assert!(!projects::summary(&scoped).unwrap().unwrap().dirty);
    assert!(!projects::summary(&scoped).unwrap().unwrap().can_undo);
    boats::rename(&scoped, "Recovered child".into()).unwrap();
    assert!(projects::summary(&state).unwrap().unwrap().dirty);
    assert!(
        projects::close(&state, false).is_err(),
        "child changes protect the entire project"
    );
    assert!(autosave::snapshot(&state, true).unwrap());
    let recovered = root.state();
    autosave::recover(&recovered, first.id, false).unwrap();
    assert_eq!(
        boats::list(&recovered).unwrap().tabs[1].name,
        "Recovered child"
    );
    assert_eq!(
        projects::summary(&state).unwrap().unwrap().id,
        first.id,
        "scoped calls always restore the root"
    );
}

#[test]
fn failed_or_stale_scoped_calls_never_target_another_boat() {
    let root = TempRoot::new("boat-context");
    let state = root.state();
    let first = projects::create(&state, "Fleet".into(), None, false).unwrap();
    let second = boats::add(&state, "B".into()).unwrap();
    let scoped = state.scoped(Some(second.id));
    assert!(
        scoped
            .with_session::<()>(|_| Err(pe_app::error::AppError::Internal("fail".into())))
            .is_err()
    );
    assert_eq!(projects::summary(&state).unwrap().unwrap().id, first.id);
    projects::create(&state, "Replacement".into(), None, true).unwrap();
    assert!(boats::rename(&scoped, "Wrong boat".into()).is_err());
}

#[test]
fn export_all_writes_separate_files_and_never_overwrites_existing_ones() {
    let root = TempRoot::new("boat-export");
    let state = root.state();
    projects::create(&state, "Fleet".into(), None, false).unwrap();
    let second = boats::add(&state, "Same / name".into()).unwrap();
    boats::rename(&state, "Same / name".into()).unwrap();
    let catalogue = pe_orc::catalogue().unwrap();
    let id = (0..catalogue.len() as u32)
        .find(|id| catalogue.entry(*id).is_some_and(|e| !e.vpp.bsp.is_empty()))
        .unwrap();
    pe_app::orc::add(&state, id, false).unwrap();
    pe_app::orc::add(&state.scoped(Some(second.id)), id, false).unwrap();
    let out = root.0.join("out");
    let first = boats::export_all(&state, &out, "csv").unwrap();
    assert!(first.failures.is_empty(), "{:?}", first.failures);
    assert_eq!(first.paths.len(), 2);
    let second = boats::export_all(&state, &out, "csv").unwrap();
    assert!(second.paths.iter().all(|p| !first.paths.contains(p)));
    assert_eq!(
        std::fs::read(&first.paths[0]).unwrap(),
        std::fs::read(&second.paths[0]).unwrap()
    );
}

fn boat(name: &str, model: Option<&str>) -> TrackerBoat {
    TrackerBoat {
        id: name.into(),
        name: name.into(),
        model: model.map(str::to_owned),
        details: BTreeMap::from([
            ("mmsi".into(), "123456789".into()),
            ("builder".into(), "Test Yard".into()),
        ]),
        sail: None,
        division: None,
        status: None,
        start: None,
        finish: None,
        fixes: vec![],
    }
}
#[test]
fn tracker_project_never_enriches_a_name_only_boat_and_preserves_full_metadata() {
    let root = TempRoot::new("tracker-fleet");
    let state = root.state();
    let catalogue = pe_orc::catalogue().unwrap();
    let entry = (0..catalogue.len() as u32)
        .filter_map(|id| catalogue.entry(id))
        .find(|e| {
            e.model
                .as_deref()
                .is_some_and(|m| boats::matching::model_key(m).is_some())
                && e.builder.is_none()
        })
        .unwrap();
    let mut matching = boat("Completely different name", entry.model.as_deref());
    matching.details.remove("builder");
    let event = TrackerEvent {
        event: EventRef {
            tracker: Tracker::YellowBrick,
            key: "fixture".into(),
            url: "https://boats.invalid/fixture".into(),
        },
        title: "Fleet race".into(),
        start: None,
        stop: None,
        positions_from: PositionsFrom::Primary,
        leg: None,
        boats: vec![boat(&entry.name, None), matching],
    };
    let (doc, reports, _) =
        boats::tracker_project::build(&state, &event, &AtomicBool::new(false)).unwrap();
    assert!(
        doc.sources.is_empty(),
        "the exact certificate boat name alone never imports a polar"
    );
    assert!(reports[0].model.is_none());
    assert!(reports[1].polars > 0);
    assert_eq!(doc.boat.details["mmsi"], "123456789");
    assert_eq!(
        doc.boat.details["trackerUrl"],
        "https://boats.invalid/fixture"
    );
    assert_eq!(doc.boat_tabs.len(), 1);
    assert!(boats::tracker_project::build(&state, &event, &AtomicBool::new(true)).is_err());
    assert!(
        projects::summary(&state).unwrap().is_none(),
        "building alone never changes the real session"
    );
}

#[test]
fn exact_boat_projects_import_polars_only_with_corroborated_identity() {
    use boats::tracker_project::{BoatMatchMode, build_with_mode};
    let root = TempRoot::new("exact-boat-polars");
    let state = root.state();
    let catalogue = pe_orc::catalogue().unwrap();
    let entry = (0..catalogue.len() as u32)
        .filter_map(|id| catalogue.entry(id))
        .find(|entry| {
            entry
                .model
                .as_deref()
                .is_some_and(|model| boats::matching::model_key(model).is_some())
                && entry
                    .builder
                    .as_ref()
                    .is_some_and(|builder| !builder.trim().is_empty())
                && entry.size[0].is_some()
                && entry.sail_no.len() >= 4
                && entry.sail_no.chars().any(|c| c.is_ascii_digit())
                && entry.sail_no.chars().any(char::is_alphabetic)
        })
        .unwrap();
    let mut vessel = boat("A renamed yacht", entry.model.as_deref());
    vessel.details = BTreeMap::from([
        ("builder".into(), entry.builder.clone().unwrap()),
        ("loa".into(), entry.size[0].unwrap().to_string()),
    ]);
    vessel.sail = Some(entry.sail_no.clone());
    let mut event = TrackerEvent {
        event: EventRef {
            tracker: Tracker::YellowBrick,
            key: "exact".into(),
            url: "https://boats.invalid/exact".into(),
        },
        title: "Exact boat race".into(),
        start: None,
        stop: None,
        positions_from: PositionsFrom::Primary,
        leg: None,
        boats: vec![vessel],
    };
    let (doc, reports, _) = build_with_mode(
        &state,
        &event,
        &AtomicBool::new(false),
        BoatMatchMode::ExactBoat,
    )
    .unwrap();
    assert!(reports[0].polars > 0);
    assert!(doc.sources.iter().all(|source| match &source.kind {
        pe_core::SourceKind::Orc { record } =>
            pe_orc::fold::compact(&record.sail_no) == pe_orc::fold::compact(&entry.sail_no),
        _ => false,
    }));
    event.boats[0].sail = None;
    event.boats[0].name = entry.name.clone();
    let (_, reports, _) = build_with_mode(
        &state,
        &event,
        &AtomicBool::new(false),
        BoatMatchMode::ExactBoat,
    )
    .unwrap();
    assert_eq!(
        reports[0].polars, 0,
        "even the name and full model specifications cannot replace an identifier"
    );
}

#[test]
fn renaming_an_originally_unnamed_boat_can_be_undone() {
    let root = TempRoot::new("boat-empty-name");
    let state = root.state();
    projects::create(&state, "Fleet".into(), None, false).unwrap();
    boats::rename(&state, "Named".into()).unwrap();
    edit::undo_last(&state).unwrap();
    assert_eq!(boats::list(&state).unwrap().tabs[0].name, "Fleet");
}

#[test]
fn deleting_and_restoring_boats_preserves_file_data_and_independent_history() {
    let root = TempRoot::new("boat-delete");
    let state = root.state();
    let first = projects::create(&state, "Fleet title".into(), None, false).unwrap();
    assert!(
        boats::remove(&state, first.id, first.id).is_err(),
        "keep the final boat"
    );
    let second = boats::add(&state, "Bravo".into()).unwrap();
    let third = boats::add(&state, "Charlie".into()).unwrap();
    boats::rename(&state.scoped(Some(second.id)), "Bravo edited".into()).unwrap();
    let catalogue = pe_orc::catalogue().unwrap();
    let id = (0..catalogue.len() as u32)
        .find(|id| catalogue.entry(*id).is_some_and(|e| !e.vpp.bsp.is_empty()))
        .unwrap();
    pe_app::orc::add(&state.scoped(Some(second.id)), id, false).unwrap();
    let path = root.file("fleet.wpsproj");
    projects::save_as(&state, path.clone()).unwrap();
    let before = state
        .with_session(|s| Ok(io::to_bytes(&s.document()?).unwrap()))
        .unwrap();
    boats::remove(&state, first.id, second.id).unwrap();
    assert!(boats::list(&state).unwrap().can_restore);
    assert_eq!(
        boats::list(&state)
            .unwrap()
            .tabs
            .iter()
            .map(|b| b.id)
            .collect::<Vec<_>>(),
        [first.id, third.id]
    );
    assert!(boats::rename(&state.scoped(Some(second.id)), "Late result".into()).is_err());
    projects::save_as(&state, path.clone()).unwrap();
    assert_eq!(
        io::from_bytes(&std::fs::read(&path).unwrap())
            .unwrap()
            .boat_tabs
            .len(),
        1
    );
    boats::restore(&state, first.id).unwrap();
    assert_eq!(
        state
            .with_session(|s| Ok(io::to_bytes(&s.document()?).unwrap()))
            .unwrap(),
        before
    );
    assert!(
        projects::summary(&state.scoped(Some(second.id)))
            .unwrap()
            .unwrap()
            .can_undo
    );

    assert!(autosave::snapshot(&state, true).unwrap());
    let promoted = boats::remove(&state, first.id, first.id).unwrap();
    assert_eq!(promoted.id, second.id);
    assert_eq!(promoted.name, "Fleet title");
    assert_eq!(promoted.path.as_deref(), Some(path.as_str()));
    assert_eq!(promoted.boat_name, "Bravo edited");
    assert!(
        autosave::list(&state)
            .iter()
            .all(|item| item.id != first.id)
    );
    assert!(
        boats::restore(&state, first.id).is_err(),
        "stale UI cannot change the new root"
    );
    assert!(autosave::snapshot(&state, true).unwrap());
    let recovered = root.state();
    autosave::recover(&recovered, second.id, false).unwrap();
    assert_eq!(boats::list(&recovered).unwrap().tabs.len(), 2);
    assert!(
        !boats::list(&recovered).unwrap().can_restore,
        "deletion history is session-local"
    );
    boats::restore(&state, second.id).unwrap();
    assert_eq!(
        state
            .with_session(|s| Ok(io::to_bytes(&s.document()?).unwrap()))
            .unwrap(),
        before
    );
    assert!(
        autosave::list(&state)
            .iter()
            .all(|item| item.id != second.id)
    );
    edit::undo_last(&state.scoped(Some(second.id))).unwrap();
    assert!(
        projects::summary(&state.scoped(Some(second.id)))
            .unwrap()
            .unwrap()
            .sources
            .is_empty()
    );
    edit::undo_last(&state.scoped(Some(second.id))).unwrap();
    assert_eq!(boats::list(&state).unwrap().tabs[1].name, "Bravo");
    boats::remove(&state, first.id, third.id).unwrap();
    projects::close(&state, true).unwrap();
    projects::create(&state, "Unrelated".into(), None, false).unwrap();
    assert!(!boats::list(&state).unwrap().can_restore);
}

#[test]
fn historical_tracks_use_full_vessel_model_fields_and_deduplicate_races() {
    let root = TempRoot::new("boat-model-tracks");
    let state = root.state();
    let path = state
        .with_session(|s| {
            s.settings.library.geojson_directory = root.0.to_string_lossy().into_owned();
            Ok(s.settings.library.metadata_path(&state))
        })
        .unwrap();
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let hit = |id: &str, vessel: &str, boat: &str, race: &str| {
        serde_json::json!({
            "id":id,"vessel_id":vessel,"participant_id":boat,"competition_id":race,
            "boat_name":"Repeated name","sail_number":"","model":"","source":"YELLOWBRICK",
            "event_name":"Historical race","original_url":format!("https://boats.invalid/{race}"),"start":null,"end":null,
            "tracker_boat_id":boat,"storage_key":""
        })
    };
    std::fs::write(path, serde_json::to_vec(&serde_json::json!({"version":1,"tables":{"Vessels":[
        {"id":"right","name":"Different name","class":"Farr 40","mmsi":"123456789"},
        {"id":"wrong","name":"Repeated name","model":"J111","mmsi":"123456789"}
    ]},"tracks":[hit("one","right","p1","race1"),hit("duplicate","right","p1","race1"),hit("two","wrong","p2","race2")]})).unwrap()).unwrap();
    for (race, participant) in [("race1", "p1"), ("race2", "p2")] {
        let data = serde_json::json!({"type":"Feature","properties":{"id":participant},"geometry":{"type":"LineString","coordinates":[[0,0,0,1753531200000u64],[0.01,0,0,1753531260000u64]]}});
        std::fs::write(
            root.file(&format!("{race}.geojson")),
            serde_json::to_vec(&data).unwrap(),
        )
        .unwrap();
    }
    let mut vessel = boat("Repeated name", Some("Farr40"));
    vessel.details.clear();
    let mut event = TrackerEvent {
        event: EventRef {
            tracker: Tracker::YellowBrick,
            key: "newrace".into(),
            url: "https://boats.invalid/newrace".into(),
        },
        title: "New race".into(),
        start: None,
        stop: None,
        positions_from: PositionsFrom::Primary,
        leg: None,
        boats: vec![vessel],
    };
    let (doc, reports, _) =
        boats::tracker_project::build(&state, &event, &AtomicBool::new(false)).unwrap();
    assert_eq!(reports[0].tracks, 1);
    assert_eq!(reports[0].missing_tracks, 0);
    let origins: Vec<_> = doc
        .sources
        .iter()
        .filter_map(|s| s.track())
        .map(|t| &t.origin)
        .collect();
    assert!(
        matches!(origins[0],pe_core::track::TrackOrigin::Tracker{event_url,..} if event_url=="https://boats.invalid/race1")
    );
    use boats::tracker_project::{BoatMatchMode, build_with_mode};
    let (_, exact, _) = build_with_mode(
        &state,
        &event,
        &AtomicBool::new(false),
        BoatMatchMode::ExactBoat,
    )
    .unwrap();
    assert_eq!(
        exact[0].tracks, 0,
        "a model match does not establish individual identity"
    );
    assert_eq!(exact[0].polars, 0);
    event.boats[0]
        .details
        .insert("mmsi".into(), "123456789".into());
    let (_, exact, _) = build_with_mode(
        &state,
        &event,
        &AtomicBool::new(false),
        BoatMatchMode::ExactBoat,
    )
    .unwrap();
    assert_eq!(
        exact[0].tracks, 1,
        "MMSI matches the correct hull but never overrides a conflicting model"
    );
    assert_eq!(exact[0].details["mmsi"], "123456789");
}

struct FixtureTracker {
    event: TrackerEvent,
    cancel: bool,
    change: Option<pe_app::commands::AppState>,
}
impl pe_trackers::TrackerClient for FixtureTracker {
    fn tracker(&self) -> Tracker {
        Tracker::YellowBrick
    }
    fn resolve(&self, _: &str) -> pe_trackers::Result<EventRef> {
        Ok(self.event.event.clone())
    }
    fn fetch_listed(
        &self,
        _: &EventRef,
        _: &pe_trackers::Fetcher,
        _: &mut dyn FnMut(pe_trackers::Progress),
        _: &mut dyn FnMut(TrackerEvent),
    ) -> pe_trackers::Result<TrackerEvent> {
        if self.cancel {
            return Err(pe_trackers::TrackerError::Cancelled);
        }
        if let Some(state) = &self.change {
            boats::rename(state, "Concurrent edit".into()).unwrap();
        }
        Ok(self.event.clone())
    }
}
#[test]
fn cancelled_or_stale_tracker_projects_never_replace_current_work() {
    let root = TempRoot::new("tracker-project-atomic");
    let state = root.state();
    let before = projects::create(&state, "Unsaved fleet".into(), None, false).unwrap();
    let event = TrackerEvent {
        event: EventRef {
            tracker: Tracker::YellowBrick,
            key: "atomic".into(),
            url: "https://boats.invalid/atomic".into(),
        },
        title: "New race".into(),
        start: None,
        stop: None,
        positions_from: PositionsFrom::Primary,
        leg: None,
        boats: vec![boat("Entrant", None)],
    };
    let client = std::sync::Arc::new(FixtureTracker {
        event: event.clone(),
        cancel: true,
        change: None,
    });
    assert!(boats::tracker_project::open_with(&state, client, "ignored", true).is_err());
    assert_eq!(projects::summary(&state).unwrap().unwrap().id, before.id);
    let client = std::sync::Arc::new(FixtureTracker {
        event: event.clone(),
        cancel: false,
        change: Some(state.clone()),
    });
    assert!(boats::tracker_project::open_with(&state, client, "ignored", true).is_err());
    assert_eq!(
        projects::summary(&state).unwrap().unwrap().boat_name,
        "Concurrent edit"
    );
    let client = std::sync::Arc::new(FixtureTracker {
        event,
        cancel: false,
        change: None,
    });
    let result = boats::tracker_project::open_with(&state, client, "ignored", true).unwrap();
    assert_ne!(result.project.id, before.id);
    assert_eq!(projects::summary(&state).unwrap().unwrap().id, before.id);
    let opened = boats::tracker_project::confirm(&state, result.project.id).unwrap();
    assert_eq!(opened.id, result.project.id);
    assert_eq!(boats::list(&state).unwrap().tabs.len(), 1);
}

/// A class chosen in the dialog opens only that class's boats, one tab each
/// (asked 2026-10-04); a class no boat sails in is refused.
#[test]
fn chosen_classes_open_only_their_boats() {
    let root = TempRoot::new("tracker-class");
    let state = root.state();
    let in_class = |name: &str, class: &str| TrackerBoat {
        division: Some(class.into()),
        ..boat(name, None)
    };
    let event = TrackerEvent {
        event: EventRef {
            tracker: Tracker::YellowBrick,
            key: "classes".into(),
            url: "https://boats.invalid/classes".into(),
        },
        title: "Race with classes".into(),
        start: None,
        stop: None,
        positions_from: PositionsFrom::Primary,
        leg: None,
        boats: vec![
            in_class("Alpha", "IRC Overall, IRC 1"),
            in_class("Bravo", "IRC Overall, IRC 2"),
            in_class("Charlie", "IRC 2"),
        ],
    };
    let open = |classes: &[&str]| {
        let client = std::sync::Arc::new(FixtureTracker {
            event: event.clone(),
            cancel: false,
            change: None,
        });
        let classes: Vec<String> = classes.iter().map(|c| (*c).to_owned()).collect();
        boats::tracker_project::open_with_mode(
            &state,
            client,
            "ignored",
            true,
            Default::default(),
            &classes,
        )
    };
    let names = |result: &boats::tracker_project::TrackerProjectResult| {
        let mut names: Vec<_> = result.boats.iter().map(|b| b.boat.clone()).collect();
        names.sort();
        names
    };
    let result = open(&["IRC 2"]).unwrap();
    assert_eq!(names(&result), ["Bravo", "Charlie"]);
    boats::tracker_project::discard_preview(&state, result.project.id).unwrap();
    // Several classes (asked 2026-10-05): the boats of any of them.
    let result = open(&["IRC 1", " IRC 2 "]).unwrap();
    assert_eq!(names(&result), ["Alpha", "Bravo", "Charlie"]);
    boats::tracker_project::discard_preview(&state, result.project.id).unwrap();
    // Each of a boat's groups is a class; a boat in two ticked ones is one
    // tab (asked 2026-10-05).
    let result = open(&["IRC Overall", "IRC 2"]).unwrap();
    assert_eq!(names(&result), ["Alpha", "Bravo", "Charlie"]);
    assert_eq!(result.boats.len(), 3, "one tab per boat, not one per class");
    boats::tracker_project::discard_preview(&state, result.project.id).unwrap();
    let result = open(&["IRC 1"]).unwrap();
    assert_eq!(names(&result), ["Alpha"]);
    boats::tracker_project::discard_preview(&state, result.project.id).unwrap();
    // No class: every boat.
    assert_eq!(open(&[]).unwrap().boats.len(), 3);
    // A class no boat sails in is refused, alone or beside one that is.
    for classes in [&["Multihull"][..], &["IRC 1", "Multihull"]] {
        let refused = open(classes).unwrap_err();
        assert_eq!(refused.kind(), "bad-option", "{refused}");
    }
}

fn preview_race(
    state: &pe_app::commands::AppState,
) -> boats::tracker_project::TrackerProjectResult {
    let client = std::sync::Arc::new(FixtureTracker {
        event: TrackerEvent {
            event: EventRef {
                tracker: Tracker::YellowBrick,
                key: "preview".into(),
                url: "https://boats.invalid/preview".into(),
            },
            title: "Preview race".into(),
            start: None,
            stop: None,
            positions_from: PositionsFrom::Primary,
            leg: None,
            boats: vec![boat("Entrant", None)],
        },
        cancel: false,
        change: None,
    });
    boats::tracker_project::open_with(state, client, "ignored", true).unwrap()
}

#[test]
fn cancelling_tracker_boat_list_preserves_fleet_history_file_and_recovery() {
    let root = TempRoot::new("tracker-preview-cancel");
    let state = root.state();
    let first = projects::create(&state, "Keep this fleet".into(), None, false).unwrap();
    let second = boats::add(&state, "Second boat".into()).unwrap();
    let path = root.file("keep.wpsproj");
    projects::save_as(&state, path.clone()).unwrap();
    let saved = std::fs::read(&path).unwrap();
    let scoped = state.scoped(Some(second.id));
    boats::rename(&scoped, "Unsaved boat name".into()).unwrap();
    autosave::snapshot(&state, true).unwrap();
    let before = state
        .with_session(|s| Ok(io::to_bytes(&s.document()?)?))
        .unwrap();
    let revision = projects::summary(&state).unwrap().unwrap().revision;

    let preview = preview_race(&state);
    assert_eq!(projects::summary(&state).unwrap().unwrap().id, first.id);
    boats::tracker_project::discard_preview(&state, preview.project.id).unwrap();
    assert!(boats::tracker_project::confirm(&state, preview.project.id).is_err());
    assert_eq!(
        before,
        state
            .with_session(|s| Ok(io::to_bytes(&s.document()?)?))
            .unwrap()
    );
    let after = projects::summary(&state).unwrap().unwrap();
    assert_eq!(after.revision, revision);
    assert!(after.dirty);
    assert_eq!(after.path, Some(path.clone()));
    assert_eq!(std::fs::read(path).unwrap(), saved);
    assert!(autosave::list(&state).iter().any(|p| p.id == first.id));
    edit::undo_last(&scoped).unwrap();
    assert_eq!(
        projects::summary(&scoped).unwrap().unwrap().boat_name,
        "Second boat"
    );
}

#[test]
fn tracker_preview_cancel_leaves_start_screen_and_stale_tokens_cannot_commit() {
    let root = TempRoot::new("tracker-preview-token");
    let state = root.state();
    let cancelled = preview_race(&state);
    assert!(projects::summary(&state).unwrap().is_none());
    boats::tracker_project::discard_preview(&state, cancelled.project.id).unwrap();
    assert!(projects::summary(&state).unwrap().is_none());
    assert!(boats::tracker_project::confirm(&state, cancelled.project.id).is_err());

    let older = preview_race(&state);
    let current = preview_race(&state);
    assert!(boats::tracker_project::confirm(&state, older.project.id).is_err());
    boats::tracker_project::discard_preview(&state, older.project.id).unwrap();
    let opened = boats::tracker_project::confirm(&state, current.project.id).unwrap();
    assert_eq!(opened.id, current.project.id);
    assert!(boats::tracker_project::confirm(&state, current.project.id).is_err());
    boats::tracker_project::discard_preview(&state, current.project.id).unwrap();
    assert_eq!(projects::summary(&state).unwrap().unwrap().id, opened.id);
}

#[test]
fn edits_while_reviewing_tracker_boats_prevent_replacement() {
    let root = TempRoot::new("tracker-preview-edit");
    let state = root.state();
    let first = projects::create(&state, "Keep this fleet".into(), None, false).unwrap();
    let second = boats::add(&state, "Second boat".into()).unwrap();
    let preview = preview_race(&state);
    let scoped = state.scoped(Some(second.id));
    boats::rename(&scoped, "Changed while reviewing".into()).unwrap();
    assert!(boats::tracker_project::confirm(&state, preview.project.id).is_err());
    boats::tracker_project::discard_preview(&state, preview.project.id).unwrap();
    assert_eq!(projects::summary(&state).unwrap().unwrap().id, first.id);
    assert_eq!(
        projects::summary(&scoped).unwrap().unwrap().boat_name,
        "Changed while reviewing"
    );
}
