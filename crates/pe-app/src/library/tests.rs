#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::json;
use std::collections::BTreeMap;

fn state(name: &str) -> AppState {
    let root = std::env::temp_dir().join(format!("pe-library-{name}-{}", uuid::Uuid::new_v4()));
    AppState::new(crate::paths::AppPaths::in_directory(&root).unwrap())
}
#[test]
fn local_search_and_import_need_no_database() {
    let state = state("search");
    let geo = state.paths.config_dir.join("tracks");
    std::fs::create_dir_all(&geo).unwrap();
    let settings = LibrarySettings {
        geojson_directory: geo.to_string_lossy().into_owned(),
        ..Default::default()
    };
    state
        .with_session(|s| {
            s.settings.library = settings.clone();
            Ok(())
        })
        .unwrap();
    let hit = catalogue::BoatTrackHit {
        id: "race/participant".into(),
        vessel_id: "vessel".into(),
        participant_id: "participant".into(),
        competition_id: "race".into(),
        boat_name: "Étoile".into(),
        sail_number: "FRA 12".into(),
        model: "Class 40".into(),
        source: "GEOVOILE".into(),
        event_name: "Ocean race".into(),
        original_url: "original race address".into(),
        start: None,
        end: None,
        tracker_boat_id: "12".into(),
        storage_key: "individual-tracks/race/vessel/provided/participant.geojson".into(),
        file_available: false,
    };
    let track = geo.join(&hit.storage_key);
    std::fs::create_dir_all(track.parent().unwrap()).unwrap();
    std::fs::write(&track,serde_json::to_vec(&json!({"type":"Feature","properties":{"vesselParticipantId":"participant","competitionUnitId":"race"},"geometry":{"type":"LineString","coordinates":[[1,50,0,1753531200000_i64,7,90,0],[1.02,50,0,1753531800000_i64,8,91,0],[1.04,50,0,1753532400000_i64,9,92,0]]}})).unwrap()).unwrap();
    let metadata = catalogue::Metadata {
        version: 1,
        tables: BTreeMap::new(),
        tracks: vec![hit],
    };
    let path = settings.metadata_path(&state);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_vec(&metadata).unwrap()).unwrap();
    let result = catalogue::search(&state, "etoile", 0).unwrap();
    assert_eq!(result.total, 1);
    assert!(result.hits[0].file_available);
    crate::projects::create(&state, "Database import".into(), None, false).unwrap();
    let imported = catalogue::import(&state, "race/participant").unwrap();
    let track = imported.project.sources[0].track.as_ref().unwrap();
    assert_eq!(track.env_status, "not_fetched");
    assert_eq!(track.samples, 3);
    assert_eq!(track.boat_name.as_deref(), Some("Étoile"));
    assert!(catalogue::import(&state, "race/participant").is_err());
    let save = state
        .paths
        .config_dir
        .join("test.wpsproj")
        .to_string_lossy()
        .into_owned();
    crate::projects::save_as(&state, save.clone()).unwrap();
    crate::projects::open(&state, save, false).unwrap();
    state.with_session(|s|{let p=&s.require_open()?.project;let pe_core::SourceKind::Track{track}=&p.sources[0].kind else{panic!("track")};assert_eq!(track.fixes[0].sog,Some(7.0));assert!(matches!(&track.origin,pe_core::track::TrackOrigin::Tracker{event_url,..} if event_url=="original race address"));Ok(())}).unwrap();
}

#[test]
fn every_vessel_value_is_searchable_from_an_existing_offline_snapshot() {
    let state = state("all-vessel-fields");
    let vessels = vec![
        json!({"id":"first","publicName":"Étoile","model":"Ocean 40","class":"IRC One","make":"Baltic","builder":"Lübeck Yachts","sailNumber":"FRA 123","length":13.72,"foiling":true,"hullColor":"red","code":"fox","notPresent":null,"customFutureColumn":{"equipment":["Carbon rig",{"designer":"Zoë Martin"},null]}}),
        json!({"id":"second","publicName":"Aster","model":"Sloop 50","class":"ORC Two","make":"Sweden","builder":"Other Yard","foiling":false}),
    ];
    let tracks = vessels
        .iter()
        .map(|v| catalogue::BoatTrackHit {
            id: format!("race/{}", v["id"].as_str().unwrap()),
            vessel_id: v["id"].as_str().unwrap().into(),
            participant_id: v["id"].as_str().unwrap().into(),
            competition_id: "race".into(),
            boat_name: v["publicName"].as_str().unwrap().into(),
            model: v["model"].as_str().unwrap().into(),
            sail_number: String::new(),
            source: "GEOVOILE".into(),
            event_name: "Event name must not match vessel search".into(),
            original_url: String::new(),
            start: None,
            end: None,
            tracker_boat_id: String::new(),
            storage_key: String::new(),
            file_available: false,
        })
        .collect();
    let metadata = catalogue::Metadata {
        version: 1,
        tables: BTreeMap::from([("Vessels".into(), vessels)]),
        tracks,
    };
    let path = LibrarySettings::default().metadata_path(&state);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let bytes = serde_json::to_vec(&metadata).unwrap();
    std::fs::write(&path, &bytes).unwrap();
    for query in [
        "etoile",
        "ocean40",
        "IRC",
        "baltic",
        "LUBECK",
        "FRA123",
        "13.72",
        "true",
        "carbon",
        "zoe martin",
        "ocean lubeck irc",
    ] {
        let found = catalogue::search(&state, query, 0).unwrap();
        assert_eq!(found.total, 1, "{query}");
        assert_eq!(found.hits[0].vessel_id, "first", "{query}");
    }
    assert_eq!(
        catalogue::search(&state, "false", 0).unwrap().hits[0].vessel_id,
        "second"
    );
    // Terms must belong to one vessel. Field names, nulls, race values, and
    // accidental concatenations across fields must never manufacture a match.
    for query in [
        "ocean sweden",
        "redfox",
        "notPresent",
        "null",
        "customFutureColumn",
        "event name",
        " / ",
    ] {
        assert_eq!(
            catalogue::search(&state, query, 0).unwrap().total,
            0,
            "{query}"
        );
    }
    assert_eq!(
        std::fs::read(path).unwrap(),
        bytes,
        "Searching must not rewrite a v1 snapshot"
    );
}

/// The database connection is gone (asked 2026-10-04): an older settings
/// file's `database` section still gives the library its folders and the
/// scraper's preferences, and nothing of the connection is kept.
#[test]
fn older_database_settings_keep_the_library_and_lose_the_connection() {
    let state = state("old-settings");
    std::fs::create_dir_all(state.paths.settings_file().parent().unwrap()).unwrap();
    std::fs::write(
        state.paths.settings_file(),
        json!({"database": {"host": "db", "password": "secret-marker", "yellowbrick_user_key": "yb-key", "yellowbrick_device_id": "yb-device",
            "geojson_directory": "/tracks", "metadata_directory": "/meta", "scrape_schedule": "startup", "scrape_urls": "https://yb.tl/race"}})
        .to_string(),
    )
    .unwrap();
    let restored = crate::settings::Settings::load(&state.paths.settings_file());
    assert_eq!(
        restored.library,
        LibrarySettings {
            geojson_directory: "/tracks".into(),
            metadata_directory: "/meta".into(),
            scrape_schedule: crate::catalogues::ScrapeSchedule::Startup,
            scrape_urls: "https://yb.tl/race".into(),
            yellowbrick_user_key: "yb-key".into(),
            yellowbrick_device_id: "yb-device".into(),
        }
    );
    restored.save(&state.paths.settings_file()).unwrap();
    let saved = std::fs::read_to_string(state.paths.settings_file()).unwrap();
    assert!(!saved.contains("secret-marker"), "{saved}");
    assert!(!saved.contains("\"database\""), "{saved}");
    // The YellowBrick key and device id never reach a log.
    let shown = format!("{:?}", restored.library);
    assert!(
        !shown.contains("yb-key") && !shown.contains("yb-device"),
        "{shown}"
    );
    let relative = LibrarySettings {
        geojson_directory: "relative".into(),
        ..Default::default()
    };
    assert!(relative.validate().is_err());
    let half = LibrarySettings {
        yellowbrick_user_key: "key".into(),
        ..Default::default()
    };
    assert!(
        half.validate().is_err(),
        "both YellowBrick fields or neither"
    );
}
