use super::LibrarySettings;
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
    sync::{Arc, Mutex},
    time::SystemTime,
};
use ts_rs::TS;

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
#[derive(Debug, Default, Serialize, Deserialize)]
pub(super) struct Metadata {
    pub version: u32,
    #[serde(default)]
    pub tables: BTreeMap<String, Vec<Value>>,
    #[serde(default)]
    pub tracks: Vec<BoatTrackHit>,
}

/// The metadata file as it is, for the scraper to add races to; an empty
/// version-1 file when there is none yet.
///
/// # Errors
/// A file that does not read, or of another version.
pub(super) fn read_metadata(state: &AppState, settings: &LibrarySettings) -> Result<Metadata> {
    let path = settings.metadata_path(state);
    if !path.is_file() {
        return Ok(Metadata {
            version: 1,
            ..Default::default()
        });
    }
    let metadata: Metadata =
        serde_json::from_slice(&std::fs::read(&path).doing("read", path.display())?)
            .doing("read", "boat metadata")?;
    if metadata.version != 1 {
        return Err(AppError::Internal(
            "This boat metadata file is of a version PolarExplorer does not read".into(),
        ));
    }
    Ok(metadata)
}

/// Writes the metadata whole, atomically, and has the next search read it again.
///
/// # Errors
/// The folder or the file not being written.
pub(super) fn write_metadata(
    state: &AppState,
    settings: &LibrarySettings,
    metadata: &Metadata,
) -> Result<()> {
    let path = settings.metadata_path(state);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).doing("create", parent.display())?;
    }
    let bytes = serde_json::to_vec(metadata).doing("encode", "boat metadata")?;
    pe_core::io::write_atomic(&path, &bytes)?;
    *state.library.lock()? = None;
    Ok(())
}

/// Search reads only vessel rows and track summaries. The other database
/// tables can be hundreds of MB and are needed only when updating metadata.
#[derive(Deserialize)]
struct SearchMetadata {
    version: u32,
    #[serde(default)]
    tables: VesselTable,
    #[serde(default)]
    tracks: Vec<BoatTrackHit>,
}
#[derive(Default, Deserialize)]
struct VesselTable {
    #[serde(default, rename = "Vessels")]
    vessels: Vec<Value>,
}

#[derive(Debug, Serialize, Deserialize)]
struct SearchGroup {
    text: String,
    details: BTreeMap<String, String>,
    tracks: Vec<usize>,
}

/// Disposable search index. Source identity includes its path, size and
/// modification time so another library or an updated snapshot cannot reuse it.
#[derive(Debug, PartialEq, Serialize, Deserialize)]
struct SourceStamp {
    path: PathBuf,
    bytes: u64,
    modified: SystemTime,
}
impl SourceStamp {
    fn read(path: &Path) -> std::io::Result<Self> {
        let info = std::fs::metadata(path)?;
        Ok(Self {
            path: path.to_owned(),
            bytes: info.len(),
            modified: info.modified()?,
        })
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct Catalogue {
    version: u32,
    source: SourceStamp,
    tracks: Vec<BoatTrackHit>,
    groups: Vec<SearchGroup>,
    #[serde(skip)]
    last_query: Mutex<Option<QueryMatches>>,
}
type QueryMatches = (Vec<String>, Arc<Vec<usize>>);
impl Catalogue {
    fn new(metadata: SearchMetadata, source: SourceStamp) -> Self {
        let mut vessels: BTreeMap<String, Value> = metadata
            .tables
            .vessels
            .into_iter()
            .map(|row| (text(&row, "id"), row))
            .collect();
        let mut group_ids = BTreeMap::new();
        let mut groups: Vec<SearchGroup> = Vec::new();
        for (i, track) in metadata.tracks.iter().enumerate() {
            let group = *group_ids.entry(&track.vessel_id).or_insert_with(|| {
                let mut value = String::new();
                let mut details = BTreeMap::new();
                if let Some(vessel) = vessels.remove(&track.vessel_id) {
                    searchable_values(&vessel, &mut value);
                    details = pe_trackers::event::boat_details(&vessel);
                } else {
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
                let id = groups.len();
                groups.push(SearchGroup {
                    text: value,
                    details,
                    tracks: Vec::new(),
                });
                id
            });
            groups[group].tracks.push(i);
        }
        Self {
            version: 1,
            source,
            tracks: metadata.tracks,
            groups,
            last_query: Mutex::default(),
        }
    }

    fn matches(&self, words: &[String]) -> Result<Arc<Vec<usize>>> {
        let mut last = self
            .last_query
            .lock()
            .map_err(|_| AppError::Internal("The track search lock was poisoned".into()))?;
        if let Some((query, hits)) = &*last
            && query == words
        {
            return Ok(Arc::clone(hits));
        }
        let mut hits = Vec::new();
        let mut scattered = Vec::new();
        if !words.is_empty() {
            // A model such as "Cal 40" must precede a name containing "cal"
            // whose timestamp or ID happens to contain "40". Keep the latter
            // searchable, but favour the complete query within one value.
            let phrase = words.concat();
            let phrase = memchr::memmem::Finder::new(phrase.as_bytes());
            let finders: Vec<_> = words
                .iter()
                .map(|w| memchr::memmem::Finder::new(w.as_bytes()))
                .collect();
            for group in &self.groups {
                if phrase.find(group.text.as_bytes()).is_some() {
                    hits.extend_from_slice(&group.tracks);
                } else if words.len() > 1
                    && finders
                        .iter()
                        .all(|f| f.find(group.text.as_bytes()).is_some())
                {
                    scattered.extend_from_slice(&group.tracks);
                }
            }
        }
        // Preserve catalogue order within each tier, including interleaved races.
        hits.sort_unstable();
        scattered.sort_unstable();
        hits.extend(scattered);
        let hits = Arc::new(hits);
        *last = Some((words.to_vec(), Arc::clone(&hits)));
        Ok(hits)
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

fn rows<'a>(tables: &'a BTreeMap<String, Vec<Value>>, name: &str) -> &'a [Value] {
    tables.get(name).map(Vec::as_slice).unwrap_or_default()
}
fn by_id<'a>(tables: &'a BTreeMap<String, Vec<Value>>, name: &str) -> BTreeMap<String, &'a Value> {
    rows(tables, name)
        .iter()
        .map(|v| (text(v, "id"), v))
        .collect()
}
/// One search record per boat in each race of the database's rows: every
/// boat of a race's group, with its track file's key when the database names
/// one, sorted by boat name and then newest first.
pub(super) fn index(tables: &BTreeMap<String, Vec<Value>>) -> Vec<BoatTrackHit> {
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
        if !super::database::SOURCES.contains(&source.to_ascii_uppercase().as_str()) {
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
fn load(state: &AppState, settings: &LibrarySettings) -> Result<Option<Arc<Catalogue>>> {
    let path = settings.metadata_path(state);
    let source = match SourceStamp::read(&path) {
        Ok(stamp) => stamp,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).doing("read", path.display()),
    };
    let mut store = state.library.lock()?;
    if let Some((_, records)) = &*store
        && records.source == source
    {
        return Ok(Some(Arc::clone(records)));
    }
    let cache = state.paths.cache_dir.join("track-search-v1.json");
    let cached = std::fs::read(&cache)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<Catalogue>(&bytes).ok())
        .filter(|c| {
            c.version == 1
                && c.source == source
                && c.groups
                    .iter()
                    .flat_map(|g| &g.tracks)
                    .all(|&i| i < c.tracks.len())
        });
    let records = if let Some(cached) = cached {
        cached
    } else {
        let metadata: SearchMetadata =
            serde_json::from_slice(&std::fs::read(&path).doing("read", path.display())?)
                .doing("read", "boat metadata")?;
        if metadata.version != 1 {
            return Err(AppError::Internal(
                "This boat metadata file is of a version PolarExplorer does not read".into(),
            ));
        }
        let records = Catalogue::new(metadata, source);
        // Cache failures never prevent searching the source. Never rewrite it.
        if SourceStamp::read(&path).ok().as_ref() == Some(&records.source)
            && let Ok(bytes) = serde_json::to_vec(&records)
        {
            let _ = pe_core::io::write_atomic(&cache, &bytes);
        }
        records
    };
    let records = Arc::new(records);
    *store = Some((path, Arc::clone(&records)));
    Ok(Some(records))
}
/// Prepare local search off the UI thread before the first keystroke. This
/// reads local files only; failures are reported if a later search needs it.
pub(crate) fn warm(state: AppState) {
    let _ = std::thread::Builder::new()
        .name("track-search-index".into())
        .spawn(move || {
            if let Ok(settings) = super::settings(&state) {
                let _ = load(&state, &settings);
            }
        });
}

/// Never let a storage key escape the selected directory, including through symlinks.
fn file_path(directory: &str, hit: &BoatTrackHit) -> Option<PathBuf> {
    let root = Path::new(directory).canonicalize().ok()?;
    if directory.is_empty() {
        return None;
    }
    file_path_in(&root, hit)
}

fn file_path_in(root: &Path, hit: &BoatTrackHit) -> Option<PathBuf> {
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
        if path.starts_with(root) && path.is_file() {
            return Some(path);
        }
    }
    None
}
/// Check presence without parsing or indexing the potentially large metadata file.
#[tauri::command]
pub async fn library_metadata_available(state: tauri::State<'_, AppState>) -> Result<bool> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || {
        Ok(super::settings(&state)?.metadata_path(&state).is_file())
    })
    .await
    .map_err(|e| AppError::Internal(format!("Track library status worker failed: {e}")))?
}

#[tauri::command]
pub async fn search_database_boats(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    query: String,
    offset: u32,
) -> Result<BoatTrackSearch> {
    let state = state.scoped(boat_context);
    tauri::async_runtime::spawn_blocking(move || search(&state, &query, offset))
        .await
        .map_err(|e| AppError::Internal(format!("Track search worker failed: {e}")))?
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
    let found = records.matches(&words)?;
    let mut hits: Vec<_> = found
        .iter()
        .skip(offset as usize)
        .take(100)
        .filter_map(|&i| records.tracks.get(i))
        .cloned()
        .collect();
    // Resolve the external-volume root once, then overlap independent file
    // checks. Import still resolves and checks the path again before reading.
    let root = if settings.geojson_directory.is_empty() {
        None
    } else {
        Path::new(&settings.geojson_directory).canonicalize().ok()
    };
    if let Some(root) = root {
        std::thread::scope(|scope| {
            for page in hits.chunks_mut(13) {
                let root = &root;
                scope.spawn(move || {
                    for hit in page {
                        hit.file_available = file_path_in(root, hit).is_some();
                    }
                });
            }
        });
    } else {
        for hit in &mut hits {
            hit.file_available = false;
        }
    }
    Ok(BoatTrackSearch {
        total: found.len() as u32,
        hits,
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
    let hit = records.tracks.iter().find(|r| r.id == id).ok_or_else(|| {
        AppError::Internal("Boat track is no longer in the local catalogue".into())
    })?;
    let project_id = state.with_session(|s| Ok(s.require_open()?.project.id))?;
    let pending = read_hit(&settings, hit)?;
    crate::tracks::add_tracks_to_project(state, vec![pending], vec![], Some(project_id))
}

pub(crate) fn read_hit(settings: &LibrarySettings, hit: &BoatTrackHit) -> Result<Pending> {
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
    let mut result = Vec::new();
    for group in &catalogue.groups {
        let Some(hit) = group.tracks.first().and_then(|&i| catalogue.tracks.get(i)) else {
            continue;
        };
        let mut details = group.details.clone();
        if !hit.model.is_empty() {
            details
                .entry("model".into())
                .or_insert_with(|| hit.model.clone());
        }
        details
            .entry("sailNumber".into())
            .or_insert_with(|| hit.sail_number.clone());
        result.push(MatchingVessel {
            profile: crate::boats::matching::Profile::from_details(&details),
            tracks: group
                .tracks
                .iter()
                .filter_map(|&i| catalogue.tracks.get(i))
                .cloned()
                .collect(),
        });
    }
    // The historical matching order is by vessel id.
    result.sort_by(|a, b| a.tracks[0].vessel_id.cmp(&b.tracks[0].vessel_id));
    Ok(result)
}
