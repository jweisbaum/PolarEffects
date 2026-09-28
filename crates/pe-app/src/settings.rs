//! Global settings, stored in `settings.json` beside the recent list
//! (spec.md 3.4). Ported from VectorEffects' `settings.rs`.
//!
//! Every field has a default and the file **grows**: a field missing from a
//! file written by an older build takes its default. The file is read field
//! by field, so one unreadable value (a hand edit, a build that wrote a unit
//! this one does not know) costs that one preference and not the recent list
//! with it. A file that is not JSON at all falls back to defaults entirely:
//! losing the preferences is far better than refusing to start.
//!
//! Settings are the person's, never the project's: no project, export or
//! history entry reads them.

use std::path::{Path, PathBuf};

use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};

/// How many recent projects are remembered (spec.md 4.4).
pub const MAX_RECENT: usize = 10;

/// The interface languages the frontend carries a catalogue for (spec.md
/// 3.5). The first is the fallback.
pub const LANGUAGES: &[&str] = &["en", "fr", "de"];

/// The theme a new install, or an unknown theme id, gets (spec.md 3.1).
pub const DEFAULT_THEME: &str = "harbour";

/// The default chunk cache size limit, in gigabytes (spec.md 3.4).
pub const DEFAULT_CACHE_LIMIT_GB: u32 = 20;
/// The largest cache limit offered: a typo of an extra zero should not ask
/// for a terabyte.
pub const MAX_CACHE_LIMIT_GB: u32 = 2_000;

/// Default number of archive requests in flight at once (spec.md 3.4).
pub const DEFAULT_CONCURRENCY: u32 = 8;
/// The most requests allowed in flight: the archives are shared public
/// buckets, and more than this is hammering them, not speeding up.
pub const MAX_CONCURRENCY: u32 = 32;
/// Default time allowed for one request, in seconds.
pub const DEFAULT_TIMEOUT_S: u32 = 60;
/// The shortest and longest request timeouts offered, in seconds.
pub const TIMEOUT_RANGE_S: (u32, u32) = (5, 600);

/// What the autosave thread does with a dirty project (spec.md 3.4, 4.5).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "AutosaveMode.ts")]
#[serde(rename_all = "snake_case")]
pub enum AutosaveMode {
    /// A crash-recovery snapshot, offered back on the start screen.
    #[default]
    Recovery,
    /// The project file itself, written in place when it has a path; a
    /// recovery snapshot when it does not.
    Save,
    /// Nothing is written until the user saves.
    Off,
}

/// Display unit for boat and wind speed. Stored values stay knots.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "SpeedUnit.ts")]
#[serde(rename_all = "snake_case")]
pub enum SpeedUnit {
    /// Knots.
    #[default]
    Kn,
    /// Metres per second.
    Ms,
    /// Kilometres per hour.
    Kmh,
}

/// Display unit for wave height. Stored values stay metres.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "WaveHeightUnit.ts")]
#[serde(rename_all = "snake_case")]
pub enum WaveHeightUnit {
    /// Metres.
    #[default]
    M,
    /// Feet.
    Ft,
}

/// Display unit for distances.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "DistanceUnit.ts")]
#[serde(rename_all = "snake_case")]
pub enum DistanceUnit {
    /// Nautical miles.
    #[default]
    Nm,
    /// Kilometres.
    Km,
}

/// The display units (spec.md 3.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "Units.ts")]
#[serde(default)]
pub struct Units {
    /// Boat and wind speed.
    pub speed: SpeedUnit,
    /// Wave height.
    pub wave_height: WaveHeightUnit,
    /// Distance.
    pub distance: DistanceUnit,
}

/// Where fetched reanalysis chunks are kept, and how much of them
/// (spec.md 3.4). Deleting the cache is always lossless (invariant 3).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "ChunkCacheSettings.ts")]
#[serde(default)]
pub struct ChunkCacheSettings {
    /// A folder chosen by the user, or empty for the platform cache
    /// directory. The chunks go in a `chunks` folder inside it, so Clear
    /// never touches anything else in a folder the user pointed at.
    pub location: String,
    /// Size limit in gigabytes.
    pub size_limit_gb: u32,
}

impl Default for ChunkCacheSettings {
    fn default() -> Self {
        Self {
            location: String::new(),
            size_limit_gb: DEFAULT_CACHE_LIMIT_GB,
        }
    }
}

/// How the reanalysis fetcher uses the network (spec.md 3.4).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "NetworkSettings.ts")]
#[serde(default)]
pub struct NetworkSettings {
    /// Requests in flight at once.
    pub concurrency: u32,
    /// Time allowed for one request, in seconds.
    pub timeout_s: u32,
}

impl Default for NetworkSettings {
    fn default() -> Self {
        Self {
            concurrency: DEFAULT_CONCURRENCY,
            timeout_s: DEFAULT_TIMEOUT_S,
        }
    }
}

/// The world map's projection (spec.md 9.1).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "MapProjection.ts")]
#[serde(rename_all = "snake_case")]
pub enum MapProjection {
    /// Plate carrée: longitude and latitude as x and y.
    #[default]
    Equirectangular,
    /// A globe seen from far away.
    Orthographic,
}

/// The settings file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[ts(export_to = "AppSettings.ts", rename = "AppSettings")]
#[serde(default)]
pub struct Settings {
    /// Most recently opened or saved projects, newest first.
    #[ts(type = "Array<string>")]
    pub recent_projects: Vec<PathBuf>,
    /// Autosave behaviour.
    pub autosave: AutosaveMode,
    /// The interface language, one of [`LANGUAGES`].
    pub language: String,
    /// The theme id, one of the bundled themes.
    pub theme: String,
    /// Display units.
    pub units: Units,
    /// The reanalysis chunk cache.
    pub chunk_cache: ChunkCacheSettings,
    /// Network use by the reanalysis fetcher.
    pub network: NetworkSettings,
    /// The map projection.
    pub projection: MapProjection,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            recent_projects: Vec::new(),
            autosave: AutosaveMode::default(),
            language: LANGUAGES[0].to_owned(),
            theme: DEFAULT_THEME.to_owned(),
            units: Units::default(),
            chunk_cache: ChunkCacheSettings::default(),
            network: NetworkSettings::default(),
            projection: MapProjection::default(),
        }
    }
}

/// The ids of the bundled themes, read from the catalogue the frontend uses,
/// so the two can never disagree about which themes exist.
pub fn known_themes() -> &'static [String] {
    static THEMES: std::sync::OnceLock<Vec<String>> = std::sync::OnceLock::new();
    THEMES.get_or_init(|| {
        let catalogue: Vec<serde_json::Value> =
            serde_json::from_str(include_str!("../../../ui/src/settings/themes.json"))
                .unwrap_or_default();
        catalogue
            .iter()
            .filter_map(|theme| theme["id"].as_str().map(str::to_owned))
            .collect()
    })
}

/// Reads `key` from a settings object into `slot`, leaving the default when
/// the key is missing or its value unreadable.
fn read_field<T: DeserializeOwned>(
    object: &serde_json::Map<String, serde_json::Value>,
    key: &str,
    slot: &mut T,
) {
    if let Some(value) = object.get(key)
        && let Ok(parsed) = serde_json::from_value(value.clone())
    {
        *slot = parsed;
    }
}

impl Settings {
    /// Reads the settings, falling back to defaults on any failure.
    pub fn load(file: &Path) -> Self {
        let value: Option<serde_json::Value> = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok());
        let mut settings = Self::default();
        if let Some(serde_json::Value::Object(object)) = value {
            read_field(&object, "recent_projects", &mut settings.recent_projects);
            read_field(&object, "autosave", &mut settings.autosave);
            read_field(&object, "language", &mut settings.language);
            read_field(&object, "theme", &mut settings.theme);
            // The nested groups are read member by member too: one unit this
            // build does not know must not reset the other two with it.
            if let Some(serde_json::Value::Object(units)) = object.get("units") {
                read_field(units, "speed", &mut settings.units.speed);
                read_field(units, "wave_height", &mut settings.units.wave_height);
                read_field(units, "distance", &mut settings.units.distance);
            }
            if let Some(serde_json::Value::Object(cache)) = object.get("chunk_cache") {
                read_field(cache, "location", &mut settings.chunk_cache.location);
                read_field(
                    cache,
                    "size_limit_gb",
                    &mut settings.chunk_cache.size_limit_gb,
                );
            }
            if let Some(serde_json::Value::Object(network)) = object.get("network") {
                read_field(network, "concurrency", &mut settings.network.concurrency);
                read_field(network, "timeout_s", &mut settings.network.timeout_s);
            }
            read_field(&object, "projection", &mut settings.projection);
        }
        settings.normalised()
    }

    /// Replaces anything out of range with its default. A file can be edited
    /// by hand or written by another build; a bad value costs one
    /// preference, never the launch.
    ///
    /// Not clamped: a limit of 0 or 500 is not a near miss of 1 or 32 but a
    /// value this build cannot vouch for, and the spec says an out-of-range
    /// value costs that preference (spec.md 3.4) — which is what the default
    /// is for.
    pub fn normalised(mut self) -> Self {
        let defaults = Self::default();
        self.recent_projects.truncate(MAX_RECENT);
        if !LANGUAGES.contains(&self.language.as_str()) {
            self.language = defaults.language;
        }
        if !known_themes().contains(&self.theme) {
            self.theme = defaults.theme;
        }
        if !(1..=MAX_CACHE_LIMIT_GB).contains(&self.chunk_cache.size_limit_gb) {
            self.chunk_cache.size_limit_gb = defaults.chunk_cache.size_limit_gb;
        }
        if !(1..=MAX_CONCURRENCY).contains(&self.network.concurrency) {
            self.network.concurrency = defaults.network.concurrency;
        }
        if !(TIMEOUT_RANGE_S.0..=TIMEOUT_RANGE_S.1).contains(&self.network.timeout_s) {
            self.network.timeout_s = defaults.network.timeout_s;
        }
        self
    }

    /// Writes the settings.
    pub fn save(&self, file: &Path) -> Result<()> {
        let json =
            serde_json::to_string_pretty(self).doing("write the settings to", file.display())?;
        if let Some(parent) = file.parent() {
            std::fs::create_dir_all(parent)
                .doing("make the settings folder at", parent.display())?;
        }
        pe_core::io::write_atomic(file, json.as_bytes())
            .doing("write the settings to", file.display())?;
        Ok(())
    }

    /// Moves `path` to the front of the recent list.
    pub fn remember(&mut self, path: &Path) {
        self.recent_projects.retain(|existing| existing != path);
        self.recent_projects.insert(0, path.to_path_buf());
        self.recent_projects.truncate(MAX_RECENT);
    }

    /// The folder the chunk cache writes into (spec.md 3.4).
    pub fn chunk_cache_dir(&self, default_cache_dir: &Path) -> PathBuf {
        let root = if self.chunk_cache.location.trim().is_empty() {
            default_cache_dir.to_path_buf()
        } else {
            PathBuf::from(self.chunk_cache.location.trim())
        };
        root.join("chunks")
    }
}

/// Applies `change` to the settings and saves them. A refused change or a
/// failed write leaves the settings exactly as they were, so what the
/// frontend shows is always what is on disk.
pub fn update(
    state: &AppState,
    change: impl FnOnce(&mut Settings) -> Result<()>,
) -> Result<Settings> {
    let file = state.paths.settings_file();
    state.with_session(|session| {
        let before = session.settings.clone();
        let saved = change(&mut session.settings).and_then(|()| session.settings.save(&file));
        match saved {
            Ok(()) => Ok(session.settings.clone()),
            Err(error) => {
                session.settings = before;
                Err(error)
            }
        }
    })
}

/// The current settings.
#[tauri::command]
pub fn app_settings(state: tauri::State<'_, AppState>) -> Result<Settings> {
    current(&state)
}

/// [`app_settings`] without a Tauri handle.
pub fn current(state: &AppState) -> Result<Settings> {
    state.with_session(|session| Ok(session.settings.clone()))
}

/// Changes the interface language and relabels the native menu.
#[tauri::command]
pub fn set_language(app: tauri::AppHandle, language: String) -> Result<Settings> {
    use tauri::Manager;
    let saved = language_set(&app.state::<AppState>(), language)?;
    crate::menu::install(&app, &saved.language);
    Ok(saved)
}

/// [`set_language`] without a Tauri handle.
pub fn language_set(state: &AppState, language: String) -> Result<Settings> {
    update(state, |settings| {
        if !LANGUAGES.contains(&language.as_str()) {
            return Err(AppError::BadOption {
                field: "language",
                value: language,
            });
        }
        settings.language = language;
        Ok(())
    })
}

/// Changes the theme.
#[tauri::command]
pub fn set_theme(state: tauri::State<'_, AppState>, theme: String) -> Result<Settings> {
    theme_set(&state, theme)
}

/// [`set_theme`] without a Tauri handle.
pub fn theme_set(state: &AppState, theme: String) -> Result<Settings> {
    update(state, |settings| {
        if !known_themes().contains(&theme) {
            return Err(AppError::BadOption {
                field: "theme",
                value: theme,
            });
        }
        settings.theme = theme;
        Ok(())
    })
}

/// Changes the display units.
#[tauri::command]
pub fn set_units(state: tauri::State<'_, AppState>, units: Units) -> Result<Settings> {
    units_set(&state, units)
}

/// [`set_units`] without a Tauri handle.
pub fn units_set(state: &AppState, units: Units) -> Result<Settings> {
    update(state, |settings| {
        settings.units = units;
        Ok(())
    })
}

/// Changes what autosave does.
#[tauri::command]
pub fn set_autosave_mode(
    state: tauri::State<'_, AppState>,
    mode: AutosaveMode,
) -> Result<Settings> {
    autosave_mode_set(&state, mode)
}

/// [`set_autosave_mode`] without a Tauri handle.
pub fn autosave_mode_set(state: &AppState, mode: AutosaveMode) -> Result<Settings> {
    update(state, |settings| {
        settings.autosave = mode;
        Ok(())
    })
}

/// Changes the chunk cache's folder and size limit.
#[tauri::command]
pub fn set_chunk_cache(
    state: tauri::State<'_, AppState>,
    cache: ChunkCacheSettings,
) -> Result<Settings> {
    chunk_cache_set(&state, cache)
}

/// [`set_chunk_cache`] without a Tauri handle. An out-of-range limit is
/// refused rather than clamped: the user typed it and should see why it did
/// not take.
pub fn chunk_cache_set(state: &AppState, cache: ChunkCacheSettings) -> Result<Settings> {
    update(state, |settings| {
        if !(1..=MAX_CACHE_LIMIT_GB).contains(&cache.size_limit_gb) {
            return Err(AppError::BadOption {
                field: "Cache size limit",
                value: cache.size_limit_gb.to_string(),
            });
        }
        let location = cache.location.trim().to_owned();
        if !location.is_empty() && !Path::new(&location).is_absolute() {
            return Err(AppError::BadOption {
                field: "Cache location",
                value: location,
            });
        }
        settings.chunk_cache = ChunkCacheSettings {
            location,
            size_limit_gb: cache.size_limit_gb,
        };
        Ok(())
    })
}

/// Changes the network settings.
#[tauri::command]
pub fn set_network(
    state: tauri::State<'_, AppState>,
    network: NetworkSettings,
) -> Result<Settings> {
    network_set(&state, network)
}

/// [`set_network`] without a Tauri handle.
pub fn network_set(state: &AppState, network: NetworkSettings) -> Result<Settings> {
    update(state, |settings| {
        if !(1..=MAX_CONCURRENCY).contains(&network.concurrency) {
            return Err(AppError::BadOption {
                field: "Concurrent requests",
                value: network.concurrency.to_string(),
            });
        }
        if !(TIMEOUT_RANGE_S.0..=TIMEOUT_RANGE_S.1).contains(&network.timeout_s) {
            return Err(AppError::BadOption {
                field: "Request timeout",
                value: network.timeout_s.to_string(),
            });
        }
        settings.network = network;
        Ok(())
    })
}

/// Changes the map projection.
#[tauri::command]
pub fn set_projection(
    state: tauri::State<'_, AppState>,
    projection: MapProjection,
) -> Result<Settings> {
    projection_set(&state, projection)
}

/// [`set_projection`] without a Tauri handle.
pub fn projection_set(state: &AppState, projection: MapProjection) -> Result<Settings> {
    update(state, |settings| {
        settings.projection = projection;
        Ok(())
    })
}

/// Where the chunk cache is and how much it holds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "ChunkCacheStatus.ts")]
pub struct ChunkCacheStatus {
    /// The folder the chunks are written to.
    pub path: String,
    /// Bytes currently in it.
    pub bytes: u64,
}

/// Total size of the regular files under `dir`; zero if it does not exist.
fn folder_size(dir: &Path) -> u64 {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return 0;
    };
    entries
        .filter_map(std::result::Result::ok)
        .map(|entry| match entry.file_type() {
            Ok(kind) if kind.is_dir() => folder_size(&entry.path()),
            Ok(kind) if kind.is_file() => entry.metadata().map_or(0, |m| m.len()),
            _ => 0,
        })
        .sum()
}

/// The chunk cache's folder and size.
#[tauri::command(async)]
pub fn chunk_cache_status(state: tauri::State<'_, AppState>) -> Result<ChunkCacheStatus> {
    cache_status(&state)
}

/// [`chunk_cache_status`] without a Tauri handle.
pub fn cache_status(state: &AppState) -> Result<ChunkCacheStatus> {
    let dir = state
        .with_session(|session| Ok(session.settings.chunk_cache_dir(&state.paths.cache_dir)))?;
    Ok(ChunkCacheStatus {
        path: dir.to_string_lossy().into_owned(),
        bytes: folder_size(&dir),
    })
}

/// Empties the chunk cache. Lossless by invariant 3: every sample a project
/// needs is saved in the project.
#[tauri::command(async)]
pub fn clear_chunk_cache(state: tauri::State<'_, AppState>) -> Result<ChunkCacheStatus> {
    cache_clear(&state)
}

/// [`clear_chunk_cache`] without a Tauri handle. Removes only the `chunks`
/// folder, never the folder the user chose to hold it.
pub fn cache_clear(state: &AppState) -> Result<ChunkCacheStatus> {
    let dir = state
        .with_session(|session| Ok(session.settings.chunk_cache_dir(&state.paths.cache_dir)))?;
    if dir.exists() {
        std::fs::remove_dir_all(&dir).doing("clear the chunk cache at", dir.display())?;
    }
    cache_status(state)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp(label: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "pe-settings-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn recent_entries_move_to_the_front_without_duplicating() {
        let mut s = Settings::default();
        s.remember(Path::new("/a"));
        s.remember(Path::new("/b"));
        s.remember(Path::new("/a"));
        assert_eq!(
            s.recent_projects,
            vec![PathBuf::from("/a"), PathBuf::from("/b")]
        );
    }

    #[test]
    fn the_recent_list_holds_ten() {
        let mut s = Settings::default();
        for i in 0..15 {
            s.remember(Path::new(&format!("/p{i}")));
        }
        assert_eq!(s.recent_projects.len(), MAX_RECENT);
        assert_eq!(s.recent_projects[0], PathBuf::from("/p14"));
    }

    /// The defaults spec.md 3.4 names.
    #[test]
    fn the_defaults_are_the_ones_the_spec_names() {
        let s = Settings::default();
        assert_eq!(s.language, "en");
        assert_eq!(s.theme, "harbour");
        assert_eq!(s.units.speed, SpeedUnit::Kn);
        assert_eq!(s.units.wave_height, WaveHeightUnit::M);
        assert_eq!(s.units.distance, DistanceUnit::Nm);
        assert_eq!(s.autosave, AutosaveMode::Recovery);
        assert_eq!(s.chunk_cache.size_limit_gb, 20);
        assert!(s.chunk_cache.location.is_empty());
        assert_eq!(s.network.concurrency, 8);
        assert_eq!(s.projection, MapProjection::Equirectangular);
        assert!(known_themes().contains(&s.theme));
        for id in ["harbour", "midnight", "ocean", "plum", "ember", "paper"] {
            assert!(known_themes().iter().any(|k| k == id), "{id}");
        }
    }

    #[test]
    fn a_broken_or_partial_file_falls_back_to_defaults() {
        let dir = temp("partial");
        let file = dir.join("settings.json");
        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(Settings::load(&file), Settings::default());

        std::fs::write(&file, r#"{ "recent_projects": ["/x.wpsproj"] }"#).unwrap();
        let partial = Settings::load(&file);
        assert_eq!(partial.recent_projects, vec![PathBuf::from("/x.wpsproj")]);
        assert_eq!(partial.autosave, AutosaveMode::Recovery);

        let mut s = Settings {
            autosave: AutosaveMode::Off,
            language: "de".to_owned(),
            theme: "paper".to_owned(),
            ..Settings::default()
        };
        s.remember(Path::new("/y.wpsproj"));
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One bad value costs that preference, not the recent list beside it.
    #[test]
    fn an_unreadable_field_loses_only_itself() {
        let dir = temp("field");
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            r#"{ "recent_projects": ["/keep.wpsproj"], "units": { "speed": "furlongs" },
                 "language": "tlh", "theme": "neon", "autosave": "save",
                 "network": { "concurrency": 500, "timeout_s": 1 },
                 "chunk_cache": { "size_limit_gb": 0 } }"#,
        )
        .unwrap();
        let s = Settings::load(&file);
        assert_eq!(s.recent_projects, vec![PathBuf::from("/keep.wpsproj")]);
        assert_eq!(s.autosave, AutosaveMode::Save);
        assert_eq!(s.units, Units::default());
        assert_eq!(s.language, "en");
        assert_eq!(s.theme, DEFAULT_THEME);
        // Out of range is the default, not the nearest allowed value.
        assert_eq!(s.network.concurrency, DEFAULT_CONCURRENCY);
        assert_eq!(s.network.timeout_s, DEFAULT_TIMEOUT_S);
        assert_eq!(s.chunk_cache.size_limit_gb, DEFAULT_CACHE_LIMIT_GB);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// One unit this build does not know resets that unit only; the other
    /// two, and the other members of the nested groups, are kept.
    #[test]
    fn each_nested_member_is_read_on_its_own() {
        let dir = temp("nested");
        let file = dir.join("settings.json");
        std::fs::write(
            &file,
            r#"{ "units": { "speed": "furlongs", "wave_height": "ft", "distance": "km" },
                 "network": { "concurrency": 12, "timeout_s": "soon" },
                 "chunk_cache": { "location": 7, "size_limit_gb": 55 } }"#,
        )
        .unwrap();
        let s = Settings::load(&file);
        assert_eq!(
            s.units,
            Units {
                speed: SpeedUnit::Kn,
                wave_height: WaveHeightUnit::Ft,
                distance: DistanceUnit::Km,
            }
        );
        assert_eq!(s.network.concurrency, 12);
        assert_eq!(s.network.timeout_s, DEFAULT_TIMEOUT_S);
        assert!(s.chunk_cache.location.is_empty());
        assert_eq!(s.chunk_cache.size_limit_gb, 55);
        let _ = std::fs::remove_dir_all(&dir);
    }

    /// The edges of each range are in range; one past them is the default.
    #[test]
    fn range_edges_are_kept_and_one_past_is_the_default() {
        let at_edges = Settings {
            chunk_cache: ChunkCacheSettings {
                location: String::new(),
                size_limit_gb: MAX_CACHE_LIMIT_GB,
            },
            network: NetworkSettings {
                concurrency: 1,
                timeout_s: TIMEOUT_RANGE_S.1,
            },
            ..Settings::default()
        };
        assert_eq!(at_edges.clone().normalised(), at_edges);
        let past = Settings {
            chunk_cache: ChunkCacheSettings {
                location: String::new(),
                size_limit_gb: MAX_CACHE_LIMIT_GB + 1,
            },
            network: NetworkSettings {
                concurrency: MAX_CONCURRENCY + 1,
                timeout_s: TIMEOUT_RANGE_S.0 - 1,
            },
            ..Settings::default()
        }
        .normalised();
        assert_eq!(past.chunk_cache.size_limit_gb, DEFAULT_CACHE_LIMIT_GB);
        assert_eq!(past.network.concurrency, DEFAULT_CONCURRENCY);
        assert_eq!(past.network.timeout_s, DEFAULT_TIMEOUT_S);
    }

    #[test]
    fn the_chunk_folder_sits_inside_the_chosen_location() {
        let mut s = Settings::default();
        assert_eq!(
            s.chunk_cache_dir(Path::new("/cache")),
            PathBuf::from("/cache/chunks")
        );
        s.chunk_cache.location = " /Volumes/big ".to_owned();
        assert_eq!(
            s.chunk_cache_dir(Path::new("/cache")),
            PathBuf::from("/Volumes/big/chunks")
        );
    }
}
