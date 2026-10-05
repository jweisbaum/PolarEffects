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
    // Known to the next scrape, and whole: no network needed for it.
    assert_eq!(
        known_urls(&metadata),
        vec!["https://yb.tl/fastnet2025".to_owned()]
    );
    let settings = LibrarySettings {
        geojson_directory: root.to_string_lossy().into_owned(),
        ..Default::default()
    };
    assert!(complete(&metadata, &settings, &event.event));
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
    // A missing file makes it incomplete again.
    std::fs::remove_file(root.join(&metadata.tracks[1].storage_key)).unwrap();
    assert!(!complete(&metadata, &settings, &event.event));
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
        "SYRF",
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
    let settings = LibrarySettings {
        geojson_directory: root.to_string_lossy().into_owned(),
        ..Default::default()
    };
    for url in [
        "https://yb.tl/fastnet2025",
        "https://race.bluewatertracks.com/2025-melbourne-hobart-westcoaster",
    ] {
        assert!(
            complete(
                &metadata,
                &settings,
                &pe_trackers::library::resolve(url).unwrap()
            ),
            "{url} is whole"
        );
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
