//! Settings and maintenance for Whirlwind's persistent, disposable chunk cache.
use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::settings::{self, Settings};
use pe_env::whirlwind::DiskCache;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use ts_rs::TS;

/// Decimal GB, as labelled in Settings.
pub const GB: u64 = 1_000_000_000;
pub const MAX_GB: u32 = 4096;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "WeatherCacheSettings.ts")]
#[serde(default)]
pub struct WeatherCacheSettings {
    /// Empty uses the platform cache directory. The cache owns a child folder.
    pub directory: String,
    /// Maximum compressed chunk storage, decimal gigabytes (1–4096).
    pub max_size_gb: u32,
}
impl Default for WeatherCacheSettings {
    fn default() -> Self {
        Self {
            directory: String::new(),
            max_size_gb: 10,
        }
    }
}
impl WeatherCacheSettings {
    pub fn directory(&self, default: &Path) -> PathBuf {
        if self.directory.is_empty() {
            default.to_owned()
        } else {
            self.directory.clone().into()
        }
    }
    pub fn max_bytes(&self) -> u64 {
        u64::from(self.max_size_gb) * GB
    }
    pub fn normalise(&mut self) {
        self.directory = self.directory.trim().to_owned();
        if !self.directory.is_empty() && !Path::new(&self.directory).is_absolute() {
            self.directory.clear();
        }
        if !(1..=MAX_GB).contains(&self.max_size_gb) {
            self.max_size_gb = Self::default().max_size_gb;
        }
    }
    fn validate(&self) -> Result<()> {
        if !self.directory.is_empty() && !Path::new(&self.directory).is_absolute() {
            return Err(AppError::BadOption {
                field: "Cache directory",
                value: self.directory.clone(),
            });
        }
        if !(1..=MAX_GB).contains(&self.max_size_gb) {
            return Err(AppError::BadOption {
                field: "Maximum cache size (GB)",
                value: self.max_size_gb.to_string(),
            });
        }
        Ok(())
    }
}

pub fn open(state: &AppState, config: &WeatherCacheSettings) -> Result<Arc<DiskCache>> {
    DiskCache::open(
        &config.directory(&state.paths.cache_dir),
        config.max_bytes(),
    )
    .map_err(AppError::from)
}

pub fn configure(state: &AppState, directory: String, max_size_gb: u32) -> Result<Settings> {
    let config = WeatherCacheSettings {
        directory: directory.trim().to_owned(),
        max_size_gb,
    };
    config.validate()?;
    // Resolve and verify the directory before accepting the setting. Disk
    // eviction is safe if saving the preference subsequently fails.
    let old = state.with_session(|s| Ok(s.settings.weather_cache.clone()))?;
    let cache = open(state, &config)?;
    match settings::update(state, |s| {
        s.weather_cache = config;
        Ok(())
    }) {
        Ok(settings) => Ok(settings),
        Err(error) => {
            let _ = cache.set_limit(old.max_bytes());
            Err(error)
        }
    }
}

#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "WeatherCacheStatus.ts")]
pub struct WeatherCacheStatus {
    pub directory: String,
    pub bytes: u64,
    pub max_bytes: u64,
}

pub fn status(state: &AppState, clear: bool) -> Result<WeatherCacheStatus> {
    let config = state.with_session(|s| Ok(s.settings.weather_cache.clone()))?;
    let cache = open(state, &config)?;
    if clear {
        cache.clear()?;
    }
    Ok(WeatherCacheStatus {
        directory: cache.directory().to_string_lossy().into_owned(),
        bytes: cache.size()?,
        max_bytes: config.max_bytes(),
    })
}

#[tauri::command]
pub async fn set_weather_cache(
    state: tauri::State<'_, AppState>,
    directory: String,
    max_size_gb: u32,
) -> Result<Settings> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || configure(&state, directory, max_size_gb))
        .await
        .map_err(|e| AppError::Internal(format!("Weather cache worker failed: {e}")))?
}
#[tauri::command]
pub async fn weather_cache_status(state: tauri::State<'_, AppState>) -> Result<WeatherCacheStatus> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || status(&state, false))
        .await
        .map_err(|e| AppError::Internal(format!("Weather cache worker failed: {e}")))?
}
#[tauri::command]
pub async fn clear_weather_cache(state: tauri::State<'_, AppState>) -> Result<WeatherCacheStatus> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || status(&state, true))
        .await
        .map_err(|e| AppError::Internal(format!("Weather cache worker failed: {e}")))?
}
