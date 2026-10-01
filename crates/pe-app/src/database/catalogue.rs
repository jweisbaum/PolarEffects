use super::{DatabaseSettings, check, connect};
use crate::{
    commands::AppState,
    error::{AppError, Context, Result},
    tracks::{Pending, TrackImportResult},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
    sync::{Arc, atomic::AtomicBool},
};
use ts_rs::TS;

pub(super) const SOURCES: &[&str] = &[
    "YELLOWBRICK",
    "GEOVOILE",
    "BLUEWATER",
    "OLDGEOVOILE",
    "GEOVOILEOLD",
    "REGADATA",
    "AMERICASCUP",
    "AMERICASCUP2021",
];
pub(super) fn text(row: &Value, field: &str) -> String {
    row[field].as_str().unwrap_or_default().to_owned()
}

#[derive(Debug, Clone, Serialize, Deserialize, TS)]
pub struct BoatTrackHit {
    pub id: String,
    pub vessel_id: String,
    pub participant_id: String,
    pub competition_id: String,
    pub boat_name: String,
    pub sail_number: String,
    pub model: String,
    pub source: String,
    pub event_name: String,
    pub original_url: String,
    pub start: Option<String>,
    pub end: Option<String>,
    pub tracker_boat_id: String,
    pub storage_key: String,
    #[serde(default)]
    pub file_available: bool,
}
#[derive(Debug, Clone, Serialize, TS)]
pub struct BoatTrackSearch {
    pub total: u32,
    pub hits: Vec<BoatTrackHit>,
    pub downloaded: bool,
}
#[derive(Debug, Serialize, Deserialize)]
pub(super) struct Metadata {
    pub version: u32,
    pub tables: BTreeMap<String, Vec<Value>>,
    pub tracks: Vec<BoatTrackHit>,
}

/// Derived only in memory, so existing version-1 downloads gain full vessel
/// search without rewriting or downloading their metadata. Tracks of the same
/// vessel share one folded string; typing never reprocesses the full JSON rows.
#[derive(Debug)]
pub(super) struct Catalogue {
    metadata: Metadata,
    search_text: Vec<Arc<str>>,
}
impl Catalogue {
    fn new(metadata: Metadata) -> Self {
        let vessels = by_id(&metadata.tables, "Vessels");
        let mut folded = BTreeMap::<&str, Arc<str>>::new();
        let search_text = metadata
            .tracks
            .iter()
            .map(|track| {
                Arc::clone(folded.entry(&track.vessel_id).or_insert_with(|| {
                    let mut value = String::new();
                    if let Some(vessel) = vessels.get(&track.vessel_id) {
                        searchable_values(vessel, &mut value);
                    } else {
                        // Keep older/minimal catalogues useful when only indexed
                        // track summaries, rather than full vessel rows, are present.
                        for field in [
                            &track.boat_name,
                            &track.model,
                            &track.sail_number,
                            &track.source,
                            &track.vessel_id,
                            &track.tracker_boat_id,
                        ] {
                            append_search_value(field, &mut value);
                        }
                    }
                    Arc::from(value)
                }))
            })
            .collect();
        Self {
            metadata,
            search_text,
        }
    }
}
fn append_search_value(value: &str, out: &mut String) {
    out.push_str(&pe_orc::fold::compact(value));
    // Don't invent a match by joining the end of one field to another.
    out.push('\0');
}
fn searchable_values(value: &Value, out: &mut String) {
    match value {
        Value::Null => {}
        Value::String(s) => append_search_value(s, out),
        Value::Bool(b) => append_search_value(if *b { "true" } else { "false" }, out),
        Value::Number(n) => append_search_value(&n.to_string(), out),
        Value::Array(values) => values.iter().for_each(|v| searchable_values(v, out)),
        Value::Object(fields) => fields.values().for_each(|v| searchable_values(v, out)),
    }
}

/// One repeatable snapshot, limited at every relationship to the requested sources.
/// Temporary keys keep the million-row source database from becoming a local copy.
pub(super) fn download(
    state: &AppState,
    settings: &DatabaseSettings,
    cancel: &AtomicBool,
) -> Result<usize> {
    check(cancel)?;
    let mut client = connect(settings)?;
    let mut tx = client
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .start()
        .doing("snapshot", "boat metadata")?;
    tx.execute("CREATE TEMP TABLE pe_events ON COMMIT DROP AS SELECT id FROM public.\"CalendarEvents\" WHERE upper(regexp_replace(source, '[^a-zA-Z0-9]', '', 'g')) = ANY($1)", &[&SOURCES]).doing("select", "supported events")?;
    tx.batch_execute(r#"
      CREATE TEMP TABLE pe_units ON COMMIT DROP AS SELECT c.id, c."vesselParticipantGroupId", c."courseId" FROM public."CompetitionUnits" c JOIN pe_events e ON e.id=c."calendarEventId";
      CREATE INDEX ON pe_units(id); CREATE INDEX ON pe_units("vesselParticipantGroupId");
      CREATE TEMP TABLE pe_vessels ON COMMIT DROP AS SELECT id FROM public."Vessels" WHERE upper(regexp_replace(source, '[^a-zA-Z0-9]', '', 'g')) IN ('YELLOWBRICK','GEOVOILE','BLUEWATER','OLDGEOVOILE','GEOVOILEOLD','REGADATA','AMERICASCUP','AMERICASCUP2021') AND "deletedAt" IS NULL;
      CREATE INDEX ON pe_vessels(id);
      CREATE TEMP TABLE pe_participants ON COMMIT DROP AS SELECT p.id FROM public."VesselParticipants" p JOIN pe_vessels v ON v.id=p."vesselId";
      CREATE INDEX ON pe_participants(id);
    "#).doing("select", "supported vessels")?;
    let filters = [
        ("Vessels", "r.id IN (SELECT id FROM pe_vessels)"),
        (
            "VesselParticipants",
            "r.id IN (SELECT id FROM pe_participants)",
        ),
        ("CalendarEvents", "r.id IN (SELECT id FROM pe_events)"),
        ("CompetitionUnits", "r.id IN (SELECT id FROM pe_units)"),
        (
            "VesselParticipantGroups",
            "r.id IN (SELECT \"vesselParticipantGroupId\" FROM pe_units)",
        ),
        (
            "VesselParticipantEvents",
            "r.\"competitionUnitId\" IN (SELECT id FROM pe_units) AND r.\"vesselParticipantId\" IN (SELECT id FROM pe_participants)",
        ),
        (
            "VesselParticipantTrackJsons",
            "r.\"competitionUnitId\" IN (SELECT id FROM pe_units) AND r.\"vesselParticipantId\" IN (SELECT id FROM pe_participants)",
        ),
        (
            "Courses",
            "r.id IN (SELECT \"courseId\" FROM pe_units) OR r.\"calendarEventId\" IN (SELECT id FROM pe_events)",
        ),
        (
            "CourseUnsequencedUntimedGeometries",
            "r.\"courseId\" IN (SELECT id FROM public.\"Courses\" WHERE id IN (SELECT \"courseId\" FROM pe_units) OR \"calendarEventId\" IN (SELECT id FROM pe_events))",
        ),
    ];
    let mut tables = BTreeMap::new();
    for (i, (table, filter)) in filters.iter().enumerate() {
        check(cancel)?;
        state.database.update(|s| {
            s.current = (*table).into();
            s.done = i as u32;
            s.total = filters.len() as u32;
        });
        let rows = tx
            .query(
                &format!(
                    "SELECT row_to_json(r) FROM public.\"{table}\" r WHERE {filter} ORDER BY r.id"
                ),
                &[],
            )
            .doing("download metadata from", table)?;
        tables.insert(
            (*table).to_owned(),
            rows.into_iter().map(|r| r.get::<_, Value>(0)).collect(),
        );
    }
    tx.commit().doing("finish", "metadata snapshot")?;
    let tracks = index(&tables);
    let count = tracks.len();
    let metadata = Metadata {
        version: 1,
        tables,
        tracks,
    };
    let path = settings.metadata_path(state);
    std::fs::create_dir_all(
        path.parent()
            .ok_or_else(|| AppError::Internal("Invalid metadata directory".into()))?,
    )
    .doing("create", path.display())?;
    let bytes = serde_json::to_vec(&metadata).doing("encode", "boat metadata")?;
    check(cancel)?;
    pe_core::io::write_atomic(&path, &bytes)?;
    state.database.lock()?.catalogue = Some((path, Arc::new(Catalogue::new(metadata))));
    Ok(count)
}
fn rows<'a>(tables: &'a BTreeMap<String, Vec<Value>>, name: &str) -> &'a [Value] {
    tables.get(name).map(Vec::as_slice).unwrap_or_default()
}
fn by_id<'a>(tables: &'a BTreeMap<String, Vec<Value>>, name: &str) -> BTreeMap<String, &'a Value> {
    rows(tables, name)
        .iter()
        .map(|v| (text(v, "id"), v))
        .collect()
}
fn index(tables: &BTreeMap<String, Vec<Value>>) -> Vec<BoatTrackHit> {
    let vessels = by_id(tables, "Vessels");
    let participants = by_id(tables, "VesselParticipants");
    let events = by_id(tables, "CalendarEvents");
    let units = by_id(tables, "CompetitionUnits");
    let mut pairs = BTreeMap::new();
    let mut groups: BTreeMap<String, Vec<&Value>> = BTreeMap::new();
    for p in participants.values() {
        let group = text(p, "vesselParticipantGroupId");
        if !group.is_empty() {
            groups.entry(group).or_default().push(p);
        }
    }
    for (id, c) in &units {
        if let Some(ps) = groups.get(&text(c, "vesselParticipantGroupId")) {
            for p in ps {
                pairs.insert((id.clone(), text(p, "id")), String::new());
            }
        }
    }
    for r in rows(tables, "VesselParticipantTrackJsons") {
        pairs.insert(
            (text(r, "competitionUnitId"), text(r, "vesselParticipantId")),
            text(r, "providedStorageKey"),
        );
    }
    for r in rows(tables, "VesselParticipantEvents") {
        pairs
            .entry((text(r, "competitionUnitId"), text(r, "vesselParticipantId")))
            .or_default();
    }
    let mut found = Vec::new();
    for ((cid, pid), key) in pairs {
        let Some(c) = units.get(&cid) else { continue };
        let Some(p) = participants.get(&pid) else {
            continue;
        };
        let Some(v) = vessels.get(&text(p, "vesselId")) else {
            continue;
        };
        let Some(e) = events.get(&text(c, "calendarEventId")) else {
            continue;
        };
        let source: String = text(e, "source")
            .chars()
            .filter(char::is_ascii_alphanumeric)
            .collect::<String>()
            .to_ascii_uppercase();
        if !SOURCES.contains(&source.to_ascii_uppercase().as_str()) {
            continue;
        }
        found.push(BoatTrackHit {
            id: format!("{cid}/{pid}"),
            vessel_id: text(v, "id"),
            participant_id: pid,
            competition_id: cid,
            boat_name: text(v, "publicName"),
            sail_number: text(v, "sailNumber"),
            model: text(v, "model"),
            source,
            event_name: if text(c, "name").is_empty() {
                text(e, "name")
            } else {
                text(c, "name")
            },
            original_url: {
                let raw = if text(c, "scrapedUrl").is_empty() {
                    text(e, "externalUrl")
                } else {
                    text(c, "scrapedUrl")
                };
                if raw.contains("://") {
                    raw
                } else {
                    pe_trackers::library::resolve_source(&text(e, "source"), &raw)
                        .map(|r| r.url)
                        .unwrap_or(raw)
                }
            },
            start: c["startTime"].as_str().map(str::to_owned),
            end: c["endTime"].as_str().map(str::to_owned),
            tracker_boat_id: text(v, "vesselId"),
            storage_key: key,
            file_available: false,
        });
    }
    found.sort_by(|a, b| {
        a.boat_name
            .to_lowercase()
            .cmp(&b.boat_name.to_lowercase())
            .then_with(|| b.start.cmp(&a.start))
            .then_with(|| a.id.cmp(&b.id))
    });
    found
}
fn load(state: &AppState, settings: &DatabaseSettings) -> Result<Option<Arc<Catalogue>>> {
    let path = settings.metadata_path(state);
    let mut store = state.database.lock()?;
    if let Some((held, records)) = &store.catalogue
        && *held == path
    {
        return Ok(Some(Arc::clone(records)));
    }
    if !path.is_file() {
        return Ok(None);
    }
    let metadata: Metadata =
        serde_json::from_slice(&std::fs::read(&path).doing("read", path.display())?)
            .doing("read", "boat metadata")?;
    if metadata.version != 1 {
        return Err(AppError::Internal(
            "Unsupported boat metadata version; download it again".into(),
        ));
    }
    let metadata = Arc::new(Catalogue::new(metadata));
    store.catalogue = Some((path, Arc::clone(&metadata)));
    Ok(Some(metadata))
}
/// Never let a database storage key escape the selected directory, including through symlinks.
fn file_path(directory: &str, hit: &BoatTrackHit) -> Option<PathBuf> {
    let root = Path::new(directory).canonicalize().ok()?;
    if directory.is_empty() {
        return None;
    }
    let mut candidates = vec![format!("{}.geojson", hit.competition_id)];
    if !hit.storage_key.is_empty() {
        candidates.push(hit.storage_key.clone());
    }
    for key in candidates {
        if Path::new(&key)
            .components()
            .any(|c| !matches!(c, std::path::Component::Normal(_)))
        {
            continue;
        }
        let Ok(path) = root.join(key).canonicalize() else {
            continue;
        };
        if path.starts_with(&root) && path.is_file() {
            return Some(path);
        }
    }
    None
}
#[tauri::command(async)]
pub fn search_database_boats(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    query: String,
    offset: u32,
) -> Result<BoatTrackSearch> {
    let state = state.scoped(boat_context);
    search(&state, &query, offset)
}
pub fn search(state: &AppState, query: &str, offset: u32) -> Result<BoatTrackSearch> {
    let settings = super::settings(state)?;
    let Some(records) = load(state, &settings)? else {
        return Ok(BoatTrackSearch {
            total: 0,
            hits: vec![],
            downloaded: false,
        });
    };
    let words = pe_orc::fold::words(query);
    let found: Vec<_> = records
        .metadata
        .tracks
        .iter()
        .zip(&records.search_text)
        .filter(|(_, text)| !words.is_empty() && words.iter().all(|w| text.contains(w)))
        .map(|(track, _)| track)
        .collect();
    Ok(BoatTrackSearch {
        total: found.len() as u32,
        hits: found
            .into_iter()
            .skip(offset as usize)
            .take(100)
            .cloned()
            .map(|mut h| {
                h.file_available = file_path(&settings.geojson_directory, &h).is_some();
                h
            })
            .collect(),
        downloaded: true,
    })
}
#[tauri::command(async)]
pub fn import_database_track(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    id: String,
) -> Result<TrackImportResult> {
    let state = state.scoped(boat_context);
    import(&state, &id)
}
pub fn import(state: &AppState, id: &str) -> Result<TrackImportResult> {
    let settings = super::settings(state)?;
    let records = load(state, &settings)?
        .ok_or_else(|| AppError::Internal("Download boat metadata in Settings first".into()))?;
    let hit = records
        .metadata
        .tracks
        .iter()
        .find(|r| r.id == id)
        .ok_or_else(|| {
            AppError::Internal("Boat track is no longer in the local catalogue".into())
        })?;
    let project_id = state.with_session(|s| Ok(s.require_open()?.project.id))?;
    let pending = read_hit(&settings, hit)?;
    crate::tracks::add_tracks_to_project(state, vec![pending], vec![], Some(project_id))
}

pub(crate) fn read_hit(settings: &DatabaseSettings, hit: &BoatTrackHit) -> Result<Pending> {
    let path = file_path(&settings.geojson_directory, hit).ok_or_else(|| {
        AppError::Internal(format!(
            "GeoJSON file missing: {}/{}.geojson",
            settings.geojson_directory, hit.competition_id
        ))
    })?;
    let bytes = std::fs::read(&path).doing("read", path.display())?;
    let ids: BTreeSet<_> = [&hit.participant_id, &hit.vessel_id, &hit.tracker_boat_id]
        .into_iter()
        .filter(|s| !s.is_empty())
        .map(String::as_str)
        .collect();
    let fixes = pe_tracks::syrf::read(&bytes, &ids).doing("read boat from", path.display())?;
    use pe_core::{
        SampleId, TrackId,
        track::{DerivationSettings, TrackOrigin, Tracker},
    };
    let tracker = match hit.source.as_str() {
        "YELLOWBRICK" => Some(Tracker::YellowBrick),
        "GEOVOILE" | "OLDGEOVOILE" | "GEOVOILEOLD" => Some(Tracker::Geovoile),
        "BLUEWATER" => Some(Tracker::BlueWaterTracks),
        _ => None,
    };
    let origin = if let Some(tracker) = tracker {
        TrackOrigin::Tracker {
            tracker,
            event_url: hit.original_url.clone(),
            event_title: hit.event_name.clone(),
            boat_id: hit.tracker_boat_id.clone(),
            boat_name: hit.boat_name.clone(),
            sail_no: Some(hit.sail_number.clone()),
            model: Some(hit.model.clone()),
            division: None,
            race_start: None,
            race_finish: None,
        }
    } else {
        TrackOrigin::File {
            name: format!("{} · {} · {}", hit.event_name, hit.original_url, hit.id),
            boat_name: Some(hit.boat_name.clone()),
        }
    };
    let mut next = 0;
    let (track, report) = pe_tracks::build_track(
        TrackId(0),
        origin,
        fixes,
        DerivationSettings::default(),
        || {
            next += 1;
            SampleId(next)
        },
    );
    Ok(Pending {
        file: path.to_string_lossy().into_owned(),
        label: format!("{} · {}", hit.boat_name, hit.event_name),
        track,
        report,
        filters: Default::default(),
    })
}

/// Full vessel fields are shared by all of its historical tracks.
pub(crate) struct MatchingVessel {
    pub profile: crate::boats::matching::Profile,
    pub tracks: Vec<BoatTrackHit>,
}
pub(crate) fn matching_vessels(state: &AppState) -> Result<Vec<MatchingVessel>> {
    let settings = super::settings(state)?;
    let Some(catalogue) = load(state, &settings)? else {
        return Ok(Vec::new());
    };
    let vessels = by_id(&catalogue.metadata.tables, "Vessels");
    let mut result = BTreeMap::<String, MatchingVessel>::new();
    for hit in &catalogue.metadata.tracks {
        result
            .entry(hit.vessel_id.clone())
            .or_insert_with(|| {
                let mut details = vessels
                    .get(&hit.vessel_id)
                    .map(|v| pe_trackers::event::boat_details(v))
                    .unwrap_or_default();
                if !hit.model.is_empty() {
                    details
                        .entry("model".into())
                        .or_insert_with(|| hit.model.clone());
                }
                details
                    .entry("sailNumber".into())
                    .or_insert_with(|| hit.sail_number.clone());
                MatchingVessel {
                    profile: crate::boats::matching::Profile::from_details(&details),
                    tracks: Vec::new(),
                }
            })
            .tracks
            .push(hit.clone());
    }
    Ok(result.into_values().collect())
}
