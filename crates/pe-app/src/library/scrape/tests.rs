use super::*;
use pe_core::track::{Fix, Tracker};

fn fix(t: i64, lon: f64, lat: f64) -> Fix {
    Fix {
        t,
        lat,
        lon,
        cog: Some(90.0),
        sog: Some(6.5),
        tws: None,
        twd_from: None,
    }
}

fn race(boats: Vec<(&str, &str, Vec<Fix>)>) -> pe_trackers::TrackerEvent {
    pe_trackers::TrackerEvent {
        event: pe_trackers::EventRef {
            tracker: Tracker::YellowBrick,
            key: "fastnet2025".into(),
            url: "https://yb.tl/fastnet2025".into(),
        },
        title: "Rolex Fastnet 2025".into(),
        start: Some(1_753_531_200),
        stop: Some(1_753_617_600),
        boats: boats
            .into_iter()
            .map(|(id, name, fixes)| pe_trackers::TrackerBoat {
                details: [("country".to_owned(), "GBR".to_owned())]
                    .into_iter()
                    .collect(),
                id: id.into(),
                name: name.into(),
                sail: Some("GBR 1".into()),
                model: Some("Class 40".into()),
                division: None,
                status: Some("FINISHED".into()),
                start: None,
                finish: None,
                fixes,
            })
            .collect(),
        positions_from: pe_trackers::event::PositionsFrom::Primary,
        leg: None,
    }
}

#[test]
fn a_scraped_race_is_track_files_and_search_records_and_a_second_scrape_replaces_them() {
    let root = std::env::temp_dir().join(format!("pe-library-scrape-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut metadata = Metadata {
        version: 1,
        ..Default::default()
    };
    let event = race(vec![
        (
            "1",
            "Alpha",
            vec![fix(0, -1.30, 50.77), fix(60, -1.20, 50.70)],
        ),
        (
            "2",
            "Bravo",
            vec![fix(0, -1.32, 50.79), fix(60, -1.40, 50.60)],
        ),
        ("3", "No track", vec![]),
    ]);
    assert_eq!(
        save_race(
            &mut metadata,
            &root,
            &event,
            "https://yb.tl/fastnet2025",
            None
        )
        .unwrap(),
        2
    );
    assert_eq!(
        metadata.tracks.len(),
        2,
        "a boat without positions has no record"
    );
    let hit = &metadata.tracks[0];
    assert_eq!(
        (
            hit.boat_name.as_str(),
            hit.source.as_str(),
            hit.event_name.as_str()
        ),
        ("Alpha", "YELLOWBRICK", "Rolex Fastnet 2025")
    );
    assert_eq!(
        hit.storage_key,
        format!(
            "individual-tracks/{}/vessel/provided/{}.geojson",
            hit.competition_id, hit.participant_id
        )
    );
    // The file is what the import reads: the boat's feature by participant id.
    let bytes = std::fs::read(root.join(&hit.storage_key)).unwrap();
    let ids = [hit.participant_id.as_str()].into_iter().collect();
    let fixes = pe_tracks::syrf::read(&bytes, &ids).unwrap();
    assert_eq!(fixes.len(), 2);
    assert_eq!(fixes[0].sog, Some(6.5));
    // The race's approximate start and end: the mean of the first and last positions.
    let unit = &metadata.tables["CompetitionUnits"][0];
    assert_eq!(
        unit["approximateStartLocation"]["coordinates"],
        json!([-1.31, 50.78])
    );
    assert_eq!(unit["scrapedUrl"], "https://yb.tl/fastnet2025");
    // Every field the tracker gives the boat is in its vessel row, so searchable.
    assert_eq!(metadata.tables["Vessels"][0]["details"]["country"], "GBR");
    // Held: the next scrape never fetches it again.
    assert!(held(&metadata).contains(&queue_key(&event.event)));
    // Scraped again: the same records, not duplicates.
    save_race(
        &mut metadata,
        &root,
        &event,
        "https://yb.tl/fastnet2025",
        None,
    )
    .unwrap();
    assert_eq!(metadata.tracks.len(), 2);
    assert_eq!(metadata.tables["Vessels"].len(), 2);
    assert_eq!(metadata.tables["CompetitionUnits"].len(), 1);
    // Still held with a file gone: a saved race is never scraped again.
    std::fs::remove_file(root.join(&metadata.tracks[1].storage_key)).unwrap();
    assert!(held(&metadata).contains(&queue_key(&event.event)));
    let _ = std::fs::remove_dir_all(root);
}

/// A snapshot's record, as SYRF wrote it: its own ids, and no file for a
/// boat the tracker had no positions for.
fn snapshot(url: &str, competition: &str, participant: &str, key: &str) -> BoatTrackHit {
    BoatTrackHit {
        id: format!("{competition}/{participant}"),
        vessel_id: format!("vessel-{participant}"),
        participant_id: participant.into(),
        competition_id: competition.into(),
        boat_name: "Boat".into(),
        sail_number: String::new(),
        model: String::new(),
        source: "YELLOWBRICK".into(),
        event_name: "Race".into(),
        original_url: url.into(),
        start: None,
        end: None,
        tracker_boat_id: participant.into(),
        storage_key: key.into(),
        file_available: !key.is_empty(),
    }
}

/// The library's own races are held whatever their files: the 682 snapshot
/// races with a boat that never had a track were fetched on every scrape
/// (2026-10-05). Addresses match as the tracker reads them (case, scheme).
#[test]
fn a_race_the_library_holds_is_held_with_or_without_files() {
    let metadata = Metadata {
        version: 1,
        tracks: vec![
            snapshot(
                "https://yb.tl/midsummersail2022",
                "syrf-1",
                "a",
                "individual-tracks/syrf-1/vessel/provided/a.geojson",
            ),
            snapshot("https://yb.tl/midsummersail2022", "syrf-1", "b", ""),
        ],
        ..Default::default()
    };
    let held = held(&metadata);
    for url in [
        "https://yb.tl/midsummersail2022",
        "http://yb.tl/MidSummerSail2022",
    ] {
        let e = pe_trackers::library::resolve(url).unwrap();
        assert!(held.contains(&queue_key(&e)), "{url}");
    }
    let other = pe_trackers::library::resolve("https://yb.tl/midsummersail2023").unwrap();
    assert!(!held.contains(&queue_key(&other)));
}

/// A race saved twice (the scraper's copy beside the snapshot's) keeps the
/// snapshot's; the scraper's records and files go. A race only the scraper
/// holds is left alone.
#[test]
fn a_duplicated_race_keeps_the_snapshot_copy() {
    let root = std::env::temp_dir().join(format!("pe-library-dup-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let mut metadata = Metadata {
        version: 1,
        tracks: vec![snapshot(
            "https://yb.tl/bunret2017",
            "syrf-2",
            "a",
            "individual-tracks/syrf-2/vessel/provided/a.geojson",
        )],
        ..Default::default()
    };
    let mut twin = race(vec![(
        "1",
        "Alpha",
        vec![fix(0, -1.30, 50.77), fix(60, -1.20, 50.70)],
    )]);
    twin.event = pe_trackers::library::resolve("https://yb.tl/bunret2017").unwrap();
    save_race(
        &mut metadata,
        &root,
        &twin,
        "https://yb.tl/bunret2017",
        None,
    )
    .unwrap();
    let only = race(vec![(
        "1",
        "Alpha",
        vec![fix(0, -1.30, 50.77), fix(60, -1.20, 50.70)],
    )]);
    save_race(
        &mut metadata,
        &root,
        &only,
        "https://yb.tl/fastnet2025",
        None,
    )
    .unwrap();
    let ours = competition_id(&twin.event);
    let file = root.join(
        &metadata
            .tracks
            .iter()
            .find(|t| t.competition_id == ours)
            .unwrap()
            .storage_key,
    );
    assert!(file.is_file());
    assert_eq!(drop_duplicates(&mut metadata, &root), 1);
    let left: Vec<&str> = metadata
        .tracks
        .iter()
        .map(|t| t.original_url.as_str())
        .collect();
    assert_eq!(
        left,
        ["https://yb.tl/bunret2017", "https://yb.tl/fastnet2025"]
    );
    assert_eq!(
        metadata.tracks[0].competition_id, "syrf-2",
        "the snapshot's copy stays"
    );
    assert!(!file.exists(), "the scraper's file is removed");
    assert!(
        !metadata.tables["CompetitionUnits"]
            .iter()
            .any(|u| u["id"] == ours.as_str())
    );
    assert_eq!(
        metadata.tables["CompetitionUnits"].len(),
        1,
        "fastnet's own unit stays"
    );
    assert_eq!(
        drop_duplicates(&mut metadata, &root),
        0,
        "nothing more to drop"
    );
    let _ = std::fs::remove_dir_all(root);
}

/// Scrapes three real races, one per tracker, into a temporary library,
/// as a scrape does: the finished-race check, the files, the records.
#[test]
#[ignore = "network; PE_TEST_LIVE=1"]
fn live_races_scrape_into_a_searchable_library() {
    if std::env::var_os("PE_TEST_LIVE").is_none() {
        return;
    }
    let root = std::env::temp_dir().join(format!("pe-library-live-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    let fetcher = pe_trackers::Fetcher::new(
        "The track library",
        Duration::from_secs(60),
        Arc::new(AtomicBool::new(false)),
    )
    .unwrap();
    let mut metadata = Metadata {
        version: 1,
        ..Default::default()
    };
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs() as i64;
    for url in [
        "https://yb.tl/fastnet2025",
        "https://24hultim.geovoile.com/2025/tracker/",
        "https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster",
    ] {
        let event = pe_trackers::library::resolve(url).unwrap();
        let client = pe_trackers::event::client(event.tracker).unwrap();
        let pe_trackers::library::completion::ScrapeFetch::Finished(race) = client
            .fetch_for_scrape(&event, &fetcher, &mut |_| {}, now)
            .unwrap()
        else {
            panic!("{url} is not finished");
        };
        let count = save_race(&mut metadata, &root, &race, url, None).unwrap();
        assert!(count > 0, "{url}");
        eprintln!("{url}: {count} tracks");
    }
    let held = held(&metadata);
    for url in [
        "https://yb.tl/fastnet2025",
        "https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster",
    ] {
        let e = pe_trackers::library::resolve(url).unwrap();
        assert!(held.contains(&queue_key(&e)), "{url} is held");
    }
    let _ = std::fs::remove_dir_all(root);
}

#[test]
fn a_storage_key_cannot_leave_the_folder() {
    let root = std::env::temp_dir().join(format!("pe-library-safe-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir_all(&root).unwrap();
    assert!(safe_file(&root, "../outside.geojson").is_err());
    assert!(safe_file(&root, "/etc/passwd").is_err());
    assert!(safe_file(&root, "individual-tracks/a/b.geojson").is_ok());
    let _ = std::fs::remove_dir_all(root);
}

/// Holding one leg of a Geovoile race holds the race's own address, and an
/// address without a leg holds the leg its name gives (2026-10-05: the
/// snapshot's "Défi Azimut 2019 - Leg 1" is the scraper's `?leg=1`).
#[test]
fn geovoile_legs_are_held_by_leg_and_by_race() {
    let mut first = snapshot(
        "http://defi-azimut.geovoile.com/2019/tracker/",
        "syrf-3",
        "a",
        "k",
    );
    first.source = "GEOVOILE".into();
    first.event_name = "Défi Azimut 2019 - Leg 1".into();
    let mut second = snapshot(
        "http://defi-azimut.geovoile.com/2017/tracker/?leg=2",
        "syrf-4",
        "a",
        "k",
    );
    second.source = "GEOVOILE".into();
    let metadata = Metadata {
        version: 1,
        tracks: vec![first, second],
        ..Default::default()
    };
    let held = held(&metadata);
    let has = |url: &str| held.contains(&queue_key(&pe_trackers::library::resolve(url).unwrap()));
    assert!(has("https://defi-azimut.geovoile.com/2019/tracker/"));
    assert!(has("https://defi-azimut.geovoile.com/2019/tracker/?leg=1"));
    assert!(!has("https://defi-azimut.geovoile.com/2019/tracker/?leg=2"));
    assert!(
        has("https://defi-azimut.geovoile.com/2017/tracker/"),
        "a leg holds the race"
    );
    assert!(has("https://defi-azimut.geovoile.com/2017/tracker/?leg=2"));
    assert!(!has("https://defi-azimut.geovoile.com/2017/tracker/?leg=1"));
}

#[test]
fn a_leg_is_read_from_the_name() {
    assert_eq!(leg_from_name("Défi Azimut 2019 - Leg 2"), Some(2));
    assert_eq!(leg_from_name("Défi Azimut (1/2)"), Some(1));
    assert_eq!(leg_from_name("Transat Jacques Vabre 2025"), None);
    assert_eq!(leg_from_name("Legacy Cup (final)"), None);
}

/// Blue Water races download first, then YellowBrick, then Geovoile (asked
/// 2026-10-05); each tracker's races keep their order.
#[test]
fn races_download_blue_water_then_yellowbrick_then_geovoile() {
    let urls = [
        "https://24hultim.geovoile.com/2025/tracker/",
        "https://yb.tl/fastnet2025",
        "https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster",
        "https://yb.tl/arc2024",
        "https://race.bluewatertracks.com/2024-melbourne-to-king-island-race",
        "https://defi-azimut.geovoile.com/2025/",
    ];
    let mut queue: Vec<(String, pe_trackers::EventRef)> = urls
        .iter()
        .map(|u| ((*u).to_owned(), pe_trackers::library::resolve(u).unwrap()))
        .collect();
    in_download_order(&mut queue);
    let order: Vec<&str> = queue.iter().map(|(u, _)| u.as_str()).collect();
    assert_eq!(
        order,
        [
            "https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster",
            "https://race.bluewatertracks.com/2024-melbourne-to-king-island-race",
            "https://yb.tl/fastnet2025",
            "https://yb.tl/arc2024",
            "https://24hultim.geovoile.com/2025/tracker/",
            "https://defi-azimut.geovoile.com/2025/",
        ]
    );
}
