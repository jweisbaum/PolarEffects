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

/// An older settings file's `database` section gives the library its
/// folders, the scraper's preferences and, since the read-only metadata
/// download came back (asked 2026-10-06), its connection.
#[test]
fn older_database_settings_keep_the_library_and_the_connection() {
    let state = state("old-settings");
    std::fs::create_dir_all(state.paths.settings_file().parent().unwrap()).unwrap();
    std::fs::write(
        state.paths.settings_file(),
        json!({"database": {"host": "db", "port": 6543, "name": "syrf", "user": "reader", "password": "secret-marker", "tls": true,
            "yellowbrick_user_key": "yb-key", "yellowbrick_device_id": "yb-device",
            "geojson_directory": "/tracks", "metadata_directory": "/meta", "scrape_schedule": "startup", "scrape_urls": "https://yb.tl/race"}})
        .to_string(),
    )
    .unwrap();
    let restored = crate::settings::Settings::load(&state.paths.settings_file());
    let expected = LibrarySettings {
        geojson_directory: "/tracks".into(),
        metadata_directory: "/meta".into(),
        scrape_schedule: crate::catalogues::ScrapeSchedule::Startup,
        scrape_urls: "https://yb.tl/race".into(),
        yellowbrick_user_key: "yb-key".into(),
        yellowbrick_device_id: "yb-device".into(),
        database: database::DatabaseConnection {
            host: "db".into(),
            port: 6543,
            name: "syrf".into(),
            user: "reader".into(),
            password: "secret-marker".into(),
            tls: true,
        },
    };
    assert_eq!(restored.library, expected);
    // Saved under `library`, and read back from there the same.
    restored.save(&state.paths.settings_file()).unwrap();
    let again = crate::settings::Settings::load(&state.paths.settings_file());
    assert_eq!(again.library, expected);
    // The password, the YellowBrick key and device id never reach a log.
    let shown = format!("{:?}", restored.library);
    for secret in ["secret-marker", "yb-key", "yb-device"] {
        assert!(!shown.contains(secret), "{shown}");
    }
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
    let nameless = database::DatabaseConnection {
        host: " ".into(),
        ..Default::default()
    };
    assert!(nameless.validate().is_err());
}

fn hit(competition: &str, participant: &str, vessel: &str, name: &str) -> catalogue::BoatTrackHit {
    catalogue::BoatTrackHit {
        id: format!("{competition}/{participant}"),
        vessel_id: vessel.into(),
        participant_id: participant.into(),
        competition_id: competition.into(),
        boat_name: name.into(),
        sail_number: String::new(),
        model: String::new(),
        source: "YELLOWBRICK".into(),
        event_name: name.into(),
        original_url: String::new(),
        start: None,
        end: None,
        tracker_boat_id: String::new(),
        storage_key: String::new(),
        file_available: false,
    }
}

/// The database's rows, as `row_to_json` gives them, for one race of two
/// boats: the second has no track file, but is in the race's group.
fn database_rows() -> BTreeMap<String, Vec<serde_json::Value>> {
    BTreeMap::from([
        (
            "CalendarEvents".into(),
            vec![
                json!({"id": "db-event", "name": "Fastnet 2025", "source": "YELLOWBRICK", "externalUrl": "https://yb.tl/fastnet2025"}),
            ],
        ),
        (
            "CompetitionUnits".into(),
            vec![
                json!({"id": "db-race", "name": "", "calendarEventId": "db-event", "vesselParticipantGroupId": "group", "scrapedUrl": "", "startTime": "2025-07-26T12:00:00Z", "endTime": null}),
            ],
        ),
        (
            "Vessels".into(),
            vec![
                json!({"id": "v-a", "publicName": "Alpha", "sailNumber": "GBR 1", "model": "JPK 1180", "vesselId": "11", "source": "YELLOWBRICK"}),
                json!({"id": "v-b", "publicName": "Bravo", "sailNumber": null, "model": null, "vesselId": "12", "source": "YELLOWBRICK"}),
            ],
        ),
        (
            "VesselParticipants".into(),
            vec![
                json!({"id": "p-a", "vesselId": "v-a", "vesselParticipantGroupId": "group"}),
                json!({"id": "p-b", "vesselId": "v-b", "vesselParticipantGroupId": "group"}),
            ],
        ),
        (
            "VesselParticipantTrackJsons".into(),
            vec![
                json!({"id": "t-a", "competitionUnitId": "db-race", "vesselParticipantId": "p-a", "providedStorageKey": "individual-tracks/db-race/vessel/provided/p-a.geojson"}),
            ],
        ),
    ])
}

/// What the download writes (asked 2026-10-06): the database's records
/// replace the database's earlier ones, and what scraping saved stays.
#[test]
fn a_download_replaces_the_databases_records_and_keeps_the_scraped_ones() {
    let scraped_event = scrape::event_id("GEOVOILE", "rhum2022");
    let existing = catalogue::Metadata {
        version: 1,
        tables: BTreeMap::from([
            (
                "CalendarEvents".into(),
                vec![
                    json!({"id": "old-db-event", "name": "Gone from the database", "source": "YELLOWBRICK"}),
                    json!({"id": scraped_event, "name": "Route du Rhum 2022", "source": "GEOVOILE", "scrapedOriginalId": "rhum2022"}),
                ],
            ),
            (
                "CompetitionUnits".into(),
                vec![
                    json!({"id": "old-db-race", "calendarEventId": "old-db-event"}),
                    json!({"id": "scraped-race", "calendarEventId": scraped_event}),
                ],
            ),
            (
                "Vessels".into(),
                vec![
                    json!({"id": "old-v", "publicName": "Old"}),
                    json!({"id": "scraped-v", "publicName": "Charlie"}),
                ],
            ),
        ]),
        tracks: vec![
            hit("old-db-race", "old-p", "old-v", "Old"),
            hit("scraped-race", "scraped-p", "scraped-v", "Charlie"),
        ],
    };
    let merged = database::merge(existing, database_rows());
    let ids = |table: &str| -> Vec<String> {
        merged.tables[table]
            .iter()
            .map(|r| r["id"].as_str().unwrap().to_owned())
            .collect()
    };
    assert_eq!(ids("CalendarEvents"), ["db-event", scraped_event.as_str()]);
    assert_eq!(ids("CompetitionUnits"), ["db-race", "scraped-race"]);
    assert_eq!(ids("Vessels"), ["v-a", "v-b", "scraped-v"]);
    let tracks: Vec<(&str, &str, &str, &str)> = merged
        .tracks
        .iter()
        .map(|t| {
            (
                t.id.as_str(),
                t.boat_name.as_str(),
                t.event_name.as_str(),
                t.storage_key.as_str(),
            )
        })
        .collect();
    assert_eq!(
        tracks,
        [
            (
                "db-race/p-a",
                "Alpha",
                "Fastnet 2025",
                "individual-tracks/db-race/vessel/provided/p-a.geojson"
            ),
            ("db-race/p-b", "Bravo", "Fastnet 2025", ""),
            ("scraped-race/scraped-p", "Charlie", "Charlie", ""),
        ]
    );
    assert_eq!(merged.tracks[0].sail_number, "GBR 1");
    assert_eq!(merged.tracks[0].original_url, "https://yb.tl/fastnet2025");
    // A second download of the same rows gives the same file.
    let again = database::merge(merged, database_rows());
    assert_eq!(again.tracks.len(), 3);
    assert_eq!(again.tables["CalendarEvents"].len(), 2);
}

/// Against a scratch database on a local server, never the person's: the
/// download reads it, writes the merged file, and the session it reads with
/// refuses a write. `PE_TEST_POSTGRES=1` and a server at localhost:5432 that
/// lets `postgres` create a database.
#[test]
#[ignore = "requires a local PostgreSQL server; PE_TEST_POSTGRES=1"]
fn the_download_reads_a_scratch_database_read_only() {
    if !std::env::var("PE_TEST_POSTGRES").is_ok_and(|v| v == "1") {
        return;
    }
    let scratch = format!("polarexplorer_metadata_test_{}", std::process::id());
    let admin = database::DatabaseConnection {
        name: "postgres".into(),
        ..Default::default()
    };
    let mut setup = postgres::Client::connect(
        &format!(
            "host={} port={} user={} dbname=postgres",
            admin.host, admin.port, admin.user
        ),
        postgres::NoTls,
    )
    .unwrap();
    setup
        .batch_execute(&format!("DROP DATABASE IF EXISTS {scratch}"))
        .unwrap();
    setup
        .batch_execute(&format!("CREATE DATABASE {scratch}"))
        .unwrap();
    let connection = database::DatabaseConnection {
        name: scratch.clone(),
        ..Default::default()
    };
    let mut fill = postgres::Client::connect(
        &format!(
            "host={} port={} user={} dbname={scratch}",
            admin.host, admin.port, admin.user
        ),
        postgres::NoTls,
    )
    .unwrap();
    fill.batch_execute(r#"
      CREATE TABLE "CalendarEvents" (id text PRIMARY KEY, name text, source text, "externalUrl" text);
      CREATE TABLE "CompetitionUnits" (id text PRIMARY KEY, name text, "calendarEventId" text, "vesselParticipantGroupId" text, "courseId" text, "scrapedUrl" text, "startTime" text, "endTime" text);
      CREATE TABLE "Vessels" (id text PRIMARY KEY, "publicName" text, "sailNumber" text, model text, "vesselId" text, source text, "deletedAt" text);
      CREATE TABLE "VesselParticipants" (id text PRIMARY KEY, "vesselId" text, "vesselParticipantGroupId" text);
      CREATE TABLE "VesselParticipantGroups" (id text PRIMARY KEY);
      CREATE TABLE "VesselParticipantEvents" (id text PRIMARY KEY, "competitionUnitId" text, "vesselParticipantId" text);
      CREATE TABLE "VesselParticipantTrackJsons" (id text PRIMARY KEY, "competitionUnitId" text, "vesselParticipantId" text, "providedStorageKey" text);
      CREATE TABLE "Courses" (id text PRIMARY KEY, "calendarEventId" text);
      CREATE TABLE "CourseUnsequencedUntimedGeometries" (id text PRIMARY KEY, "courseId" text);
      INSERT INTO "CalendarEvents" VALUES ('db-event', 'Fastnet 2025', 'YellowBrick', 'https://yb.tl/fastnet2025'), ('other', 'Club night', 'Manual', '');
      INSERT INTO "CompetitionUnits" VALUES ('db-race', '', 'db-event', 'group', NULL, '', '2025-07-26T12:00:00Z', NULL), ('other-race', 'Club', 'other', 'other-group', NULL, '', NULL, NULL);
      INSERT INTO "Vessels" VALUES ('v-a', 'Alpha', 'GBR 1', 'JPK 1180', '11', 'YELLOWBRICK', NULL), ('v-b', 'Bravo', NULL, NULL, '12', 'YELLOWBRICK', NULL),
        ('v-gone', 'Deleted', NULL, NULL, '13', 'YELLOWBRICK', '2025-01-01'), ('v-other', 'Dinghy', NULL, NULL, '14', 'MANUAL', NULL);
      INSERT INTO "VesselParticipants" VALUES ('p-a', 'v-a', 'group'), ('p-b', 'v-b', 'group'), ('p-gone', 'v-gone', 'group'), ('p-other', 'v-other', 'other-group');
      INSERT INTO "VesselParticipantGroups" VALUES ('group'), ('other-group');
      INSERT INTO "VesselParticipantTrackJsons" VALUES ('t-a', 'db-race', 'p-a', 'individual-tracks/db-race/vessel/provided/p-a.geojson');
    "#).unwrap();
    drop(fill);

    let state = state("postgres");
    let settings = LibrarySettings {
        database: connection.clone(),
        ..Default::default()
    };
    assert_eq!(database::test_connection(&connection).unwrap(), scratch);
    let mut seen = Vec::new();
    let (tracks, kept) = database::download(
        &state,
        &settings,
        &std::sync::atomic::AtomicBool::new(false),
        |_, table| seen.push(table.to_owned()),
    )
    .unwrap();
    assert_eq!((tracks, kept), (2, 0));
    assert_eq!(seen.len(), 9);
    let written: catalogue::Metadata =
        serde_json::from_slice(&std::fs::read(settings.metadata_path(&state)).unwrap()).unwrap();
    let names: Vec<&str> = written
        .tracks
        .iter()
        .map(|t| t.boat_name.as_str())
        .collect();
    assert_eq!(names, ["Alpha", "Bravo"], "no deleted or unsupported boats");
    assert_eq!(written.tables["Vessels"].len(), 2);
    assert_eq!(written.tables["CalendarEvents"].len(), 1);
    assert_eq!(catalogue::search(&state, "alpha", 0).unwrap().total, 1);

    // The session the download reads with refuses a write, temporary or not.
    let mut session = database::connect(&connection).unwrap();
    for statement in [
        r#"INSERT INTO "Vessels" (id) VALUES ('written')"#,
        "CREATE TEMP TABLE written AS SELECT 1",
    ] {
        let error = session.batch_execute(statement).unwrap_err();
        let message = error.as_db_error().map(|e| e.message().to_owned());
        assert!(
            message
                .as_deref()
                .is_some_and(|m| m.contains("read-only transaction")),
            "{error:?}"
        );
    }
    drop(session);
    setup
        .batch_execute(&format!("DROP DATABASE {scratch} WITH (FORCE)"))
        .unwrap();
}
