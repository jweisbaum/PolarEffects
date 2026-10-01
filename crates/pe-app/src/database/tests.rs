#![allow(clippy::unwrap_used, clippy::expect_used)]
use super::*;
use serde_json::json;
use std::{collections::BTreeMap, sync::atomic::AtomicBool};

fn state(name: &str) -> AppState {
    let root = std::env::temp_dir().join(format!("pe-database-{name}-{}", uuid::Uuid::new_v4()));
    AppState::new(crate::paths::AppPaths::in_directory(&root).unwrap())
}
#[test]
fn local_search_import_weather_and_undo_do_not_need_postgres() {
    let state = state("search");
    let geo = state.paths.config_dir.join("tracks");
    std::fs::create_dir_all(&geo).unwrap();
    let settings = DatabaseSettings {
        geojson_directory: geo.to_string_lossy().into_owned(),
        ..Default::default()
    };
    state
        .with_session(|s| {
            s.settings.database = settings.clone();
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
fn password_is_not_debugged_and_preferences_survive_old_settings() {
    let settings = DatabaseSettings {
        password: "secret-marker".into(),
        yellowbrick_user_key: "yb-secret-marker".into(),
        yellowbrick_device_id: "device-secret-marker".into(),
        ..Default::default()
    };
    assert!(!format!("{settings:?}").contains("secret-marker"));
    settings.validate().unwrap();
    let mut missing_device = settings.clone();
    missing_device.yellowbrick_device_id.clear();
    assert!(missing_device.validate().is_err());
    let state = state("yb-settings");
    let preferences = crate::settings::Settings {
        database: settings,
        ..Default::default()
    };
    preferences.save(&state.paths.settings_file()).unwrap();
    let restored = crate::settings::Settings::load(&state.paths.settings_file());
    assert_eq!(restored.database.yellowbrick_user_key, "yb-secret-marker");
    assert_eq!(
        restored.database.yellowbrick_device_id,
        "device-secret-marker"
    );
    assert_eq!(
        serde_json::from_value::<DatabaseSettings>(json!({"host":"db"}))
            .unwrap()
            .scrape_schedule,
        ScrapeSchedule::OnDemand
    );
}

#[test]
#[ignore = "requires isolated schema clone polareffects_yb_catalogue_test"]
fn yellowbrick_children_share_parent_and_case_aliases_do_not_duplicate() {
    let state = state("yb-parent");
    let settings = DatabaseSettings {
        name: "polareffects_yb_catalogue_test".into(),
        geojson_directory: state
            .paths
            .config_dir
            .join("tracks")
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    let mut db = connect(&settings).unwrap();
    let existing = "27b193d2-0cd9-4547-8bab-93b3663c759f";
    db.execute(r#"INSERT INTO public."CalendarEvents" (id,name,source,"externalUrl","scrapedOriginalId","isPrivate","createdAt","updatedAt") VALUES ($1::text::uuid,'Original name','YELLOWBRICK','https://yb.tl/original-family','9992030',true,now(),now())"#, &[&existing]).unwrap();
    for id in ["9992030", "9992031"] {
        let family = pe_trackers::library::yellowbrick::Race {
            id: id.into(),
            title: format!("Family {id}"),
            date: "2030-01-01".into(),
            urls: vec![],
        };
        for leg in [1, 2, 1] {
            let url = format!("https://yb.tl/Fixture{id}_{leg}");
            let mut event = pe_trackers::TrackerEvent {
                event: pe_trackers::library::resolve(&url).unwrap(),
                title: format!("Leg {leg}"),
                start: Some(1753531200),
                stop: Some(1753532400),
                positions_from: pe_trackers::event::PositionsFrom::Primary,
                leg: None,
                boats: vec![],
            };
            ingest::save(
                &settings,
                &event,
                &url,
                &[],
                Some(&family),
                &AtomicBool::new(false),
            )
            .unwrap();
            event.event = pe_trackers::library::resolve(&url.to_lowercase()).unwrap();
            ingest::save(
                &settings,
                &event,
                &url.to_lowercase(),
                &[],
                Some(&family),
                &AtomicBool::new(false),
            )
            .unwrap();
        }
        let rows = db.query(r#"SELECT e.id::text,e.name,e."externalUrl",c."scrapedUrl" FROM public."CalendarEvents" e JOIN public."CompetitionUnits" c ON c."calendarEventId"=e.id WHERE e.source='YELLOWBRICK' AND e."scrapedOriginalId"=$1 ORDER BY c."scrapedUrl""#, &[&id]).unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0].get::<_, String>(0), rows[1].get::<_, String>(0));
        assert_eq!(
            rows[0].get::<_, String>(3),
            format!("https://yb.tl/Fixture{id}_1")
        );
        if id == "9992030" {
            assert_eq!(rows[0].get::<_, String>(0), existing);
            assert_eq!(rows[0].get::<_, String>(1), "Original name");
            assert_eq!(rows[0].get::<_, String>(2), "https://yb.tl/original-family");
        } else {
            assert_eq!(rows[0].get::<_, String>(1), family.title);
        }
    }
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
    let path = DatabaseSettings::default().metadata_path(&state);
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

/// Read-only verification against the user's explicitly configured database.
#[test]
#[ignore = "requires local syrfbackendprod and mounted tracks"]
fn local_database_snapshot_matches_real_individual_tracks() {
    let state = state("live-read");
    let settings = DatabaseSettings::default();
    state
        .with_session(|s| {
            s.settings.database = settings.clone();
            Ok(())
        })
        .unwrap();
    let count = catalogue::download(&state, &settings, &AtomicBool::new(false)).unwrap();
    assert!(count > 1000);
    let result = catalogue::search(&state, "Lurline", 0).unwrap();
    assert!(result.total > 0);
    assert!(result.hits.iter().any(|h| h.file_available));
    let hit = result.hits.iter().find(|h| h.file_available).unwrap();
    crate::projects::create(&state, "Real database track".into(), None, false).unwrap();
    let imported = catalogue::import(&state, &hit.id).unwrap();
    assert!(imported.imported[0].fixes > 10);
    eprintln!(
        "Downloaded {count} searchable tracks; imported {} fixes from {}",
        imported.imported[0].fixes, hit.boat_name
    );
}

#[test]
#[ignore = "requires isolated schema clone named polareffects_syrf_test"]
fn isolated_database_ingestion_is_repeatable_and_export_restores() {
    let state = state("ingest");
    let geo = state.paths.config_dir.join("tracks");
    let settings = DatabaseSettings {
        name: "polareffects_syrf_test".into(),
        geojson_directory: geo.to_string_lossy().into_owned(),
        ..Default::default()
    };
    state
        .with_session(|s| {
            s.settings.database = settings.clone();
            Ok(())
        })
        .unwrap();
    use pe_core::track::{Fix, Tracker};
    let event = pe_trackers::TrackerEvent {
        event: pe_trackers::EventRef {
            tracker: Tracker::YellowBrick,
            key: "polareffects-test-race".into(),
            url: "polareffects-test-race".into(),
        },
        title: "Test ocean race".into(),
        start: Some(1753531200),
        stop: Some(1753532400),
        positions_from: pe_trackers::event::PositionsFrom::Primary,
        leg: None,
        boats: vec![pe_trackers::TrackerBoat {
            details: Default::default(),
            id: "12".into(),
            name: "Test boat".into(),
            sail: Some("USA 12".into()),
            model: Some("Class 40".into()),
            division: None,
            status: Some("FINISHED".into()),
            start: Some(1753531200),
            finish: Some(1753532400),
            fixes: vec![
                Fix {
                    tws: None,
                    twd_from: None,
                    t: 1753531200,
                    lat: 50.0,
                    lon: 1.0,
                    cog: Some(90.0),
                    sog: Some(7.0),
                },
                Fix {
                    tws: None,
                    twd_from: None,
                    t: 1753532400,
                    lat: 50.0,
                    lon: 1.1,
                    cog: Some(90.0),
                    sog: Some(8.0),
                },
            ],
        }],
    };
    let cancel = AtomicBool::new(false);
    let course = vec![
        json!({"type":"Feature","geometry":{"type":"Point","coordinates":[1,50]},"properties":{"name":"Start"}}),
    ];
    assert_eq!(
        ingest::save(
            &settings,
            &event,
            "polareffects-test-race",
            &course,
            None,
            &cancel
        )
        .unwrap(),
        1
    );
    assert_eq!(
        ingest::save(
            &settings,
            &event,
            "polareffects-test-race",
            &course,
            None,
            &cancel
        )
        .unwrap(),
        1
    );
    assert_eq!(catalogue::download(&state, &settings, &cancel).unwrap(), 1);
    let found = catalogue::search(&state, "Test boat", 0).unwrap();
    assert_eq!(found.total, 1);
    assert!(found.hits[0].file_available);
    let mut client = connect(&settings).unwrap();
    for table in [
        "Vessels",
        "VesselParticipants",
        "CalendarEvents",
        "CompetitionUnits",
        "VesselParticipantTrackJsons",
    ] {
        let count: i64 = client
            .query_one(&format!("SELECT count(*) FROM public.\"{table}\""), &[])
            .unwrap()
            .get(0);
        assert_eq!(count, 1, "{table}");
    }
    let count: i64 = client
        .query_one(
            "SELECT count(*) FROM public.\"VesselParticipantEvents\"",
            &[],
        )
        .unwrap()
        .get(0);
    assert_eq!(count, 0);
    let path = std::env::temp_dir().join("polareffects-syrf-restore-test.sql");
    export::run(&settings, &path, &cancel).unwrap();
    let sql = std::fs::read_to_string(path).unwrap();
    assert!(sql.contains("COPY public.\"Vessels\""));
    assert!(sql.contains("CREATE EXTENSION"));
    assert!(!sql.contains("OWNER TO"));
    assert!(!sql.contains("GRANT ALL"));
}

/// Exercises the actual provider decoders, course extraction, SQL writes and
/// local import together. This never writes to the user's production database.
#[test]
#[ignore = "network and isolated schema clone polareffects_syrf_live_test; PE_TEST_LIVE=1"]
fn live_scrapers_save_all_three_sources_without_duplicates() {
    if !std::env::var("PE_TEST_LIVE").is_ok_and(|v| v == "1") {
        return;
    }
    let state = state("live-scrapers");
    let settings = DatabaseSettings {
        name: "polareffects_syrf_live_test".into(),
        geojson_directory: state
            .paths
            .config_dir
            .join("tracks")
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    state
        .with_session(|s| {
            s.settings.database = settings.clone();
            Ok(())
        })
        .unwrap();
    let cancel = std::sync::Arc::new(AtomicBool::new(false));
    let fetcher =
        pe_trackers::Fetcher::new("SYRF", std::time::Duration::from_secs(60), cancel.clone())
            .unwrap();
    let mut db = connect(&settings).unwrap();
    crate::projects::create(&state, "Native scraper validation".into(), None, false).unwrap();
    for url in [
        "https://yb.tl/fastnet2025",
        "https://24hultim.geovoile.com/2025/tracker/",
        "https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster",
    ] {
        let event = pe_trackers::library::resolve(url).unwrap();
        let client = pe_trackers::event::client(event.tracker).unwrap();
        let mut event = client.fetch(&event, &fetcher, &mut |_| {}).unwrap();
        let course = pe_trackers::library::course(&fetcher, &mut event).unwrap();
        let tracks = event.boats.iter().filter(|b| !b.fixes.is_empty()).count();
        assert!(tracks > 0, "{url}");
        let counts = |db: &mut postgres::Client| -> Vec<i64> {
            [
                "CalendarEvents",
                "CompetitionUnits",
                "Vessels",
                "VesselParticipants",
                "VesselParticipantTrackJsons",
                "CourseUnsequencedUntimedGeometries",
            ]
            .iter()
            .map(|t| {
                db.query_one(&format!("SELECT count(*) FROM public.\"{t}\""), &[])
                    .unwrap()
                    .get(0)
            })
            .collect()
        };
        assert_eq!(
            ingest::save(&settings, &event, url, &course, None, &cancel).unwrap(),
            tracks
        );
        let before = counts(&mut db);
        assert_eq!(
            ingest::save(&settings, &event, url, &course, None, &cancel).unwrap(),
            tracks
        );
        assert_eq!(
            counts(&mut db),
            before,
            "Repeated scrape duplicated rows: {url}"
        );
        catalogue::download(&state, &settings, &cancel).unwrap();
        let boat = event.boats.iter().find(|b| b.fixes.len() > 2).unwrap();
        let found = catalogue::search(&state, &boat.name, 0).unwrap();
        let hit = found.hits.iter().find(|h| h.original_url == url).unwrap();
        assert!(hit.file_available);
        let imported = catalogue::import(&state, &hit.id).unwrap();
        assert!(imported.imported[0].fixes > 2);
        let bytes =
            std::fs::read(std::path::Path::new(&settings.geojson_directory).join(&hit.storage_key))
                .unwrap();
        let geo: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(geo["type"], "Feature");
        assert_eq!(geo["properties"]["vesselParticipantId"], hit.participant_id);
        assert_eq!(geo["properties"]["competitionUnitId"], hit.competition_id);
        assert_eq!(geo["geometry"]["coordinates"][0][3], boat.fixes[0].t * 1000);
        eprintln!(
            "Native scrape {url}: {tracks} tracks, {} course geometries; repeat and import passed",
            course.len()
        );
    }
}

#[test]
#[ignore = "requires isolated schema clone polareffects_syrf_live_test"]
fn new_leg_reuses_upstream_calendar_guid_and_preserves_provenance() {
    let state = state("legacy-leg");
    let settings = DatabaseSettings {
        name: "polareffects_syrf_live_test".into(),
        geojson_directory: state
            .paths
            .config_dir
            .join("tracks")
            .to_string_lossy()
            .into_owned(),
        ..Default::default()
    };
    let mut db = connect(&settings).unwrap();
    let eid = "96972980-4a4b-480b-a04c-5f4c0174a07f";
    let cid = "4279db28-1009-4aac-8d60-bb1dc0c02b31";
    let first_url = "http://polareffects-validation.geovoile.com/2030/tracker/?leg=1";
    db.execute(r#"INSERT INTO public."CalendarEvents" (id,name,source,"externalUrl","scrapedOriginalId","isPrivate","approximateStartTime","createdAt","updatedAt") VALUES ($1::text::uuid,'Original event','GEOVOILE',$2,'upstream-id',true,'2025-01-01T00:00:00Z',now(),now()) ON CONFLICT (id) DO NOTHING"#, &[&eid,&first_url]).unwrap();
    db.execute(r#"INSERT INTO public."CompetitionUnits" (id,"calendarEventId","scrapedUrl","scrapedOriginalId","createdAt","updatedAt") VALUES ($1::text::uuid,$2::text::uuid,$3,'upstream-leg-1',now(),now()) ON CONFLICT (id) DO NOTHING"#, &[&cid,&eid,&first_url]).unwrap();
    let second_url = "https://polareffects-validation.geovoile.com/2030/tracker/?leg=2";
    let event = pe_trackers::TrackerEvent {
        event: pe_trackers::library::resolve(second_url).unwrap(),
        title: "Leg 2".into(),
        start: Some(1753531200),
        stop: Some(1753532400),
        positions_from: pe_trackers::event::PositionsFrom::Primary,
        leg: Some((2, 2)),
        boats: vec![],
    };
    ingest::save(
        &settings,
        &event,
        second_url,
        &[],
        None,
        &AtomicBool::new(false),
    )
    .unwrap();
    let row = db.query_one(r#"SELECT c."calendarEventId"::text,e.name,e."externalUrl",e."scrapedOriginalId",e."isPrivate",extract(epoch from e."approximateStartTime")::bigint FROM public."CompetitionUnits" c JOIN public."CalendarEvents" e ON e.id=c."calendarEventId" WHERE c."scrapedUrl"=$1"#, &[&second_url]).unwrap();
    assert_eq!(row.get::<_, String>(0), eid);
    assert_eq!(row.get::<_, String>(1), "Original event");
    assert_eq!(row.get::<_, String>(2), first_url);
    assert_eq!(row.get::<_, String>(3), "upstream-id");
    assert!(row.get::<_, bool>(4));
    assert_eq!(row.get::<_, i64>(5), 1735689600);
}
