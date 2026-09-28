//! Application directory layout (spec.md 3.4). Ported from VectorEffects.
//!
//! Settings and the recent list live in the platform config directory,
//! crash-recovery snapshots in the data directory, and the reanalysis chunk
//! cache under the cache directory. Deleting the cache directory while the
//! app is closed is always lossless (invariant 3).

use std::path::{Path, PathBuf};

use directories::ProjectDirs;

use crate::error::{AppError, Context, Result};

/// Resolved locations for everything the app writes outside a project file.
#[derive(Debug, Clone)]
pub struct AppPaths {
    /// User settings and the recent list.
    pub config_dir: PathBuf,
    /// Evictable caches. Safe to delete when the app is closed.
    pub cache_dir: PathBuf,
    /// Crash-recovery snapshots (spec.md 4.5).
    pub autosave_dir: PathBuf,
    /// Log files.
    pub log_dir: PathBuf,
}

impl AppPaths {
    /// Resolves the platform directories, creating them if needed.
    pub fn resolve() -> Result<Self> {
        let dirs = ProjectDirs::from("com", "PolarEffects", "PolarEffects")
            .ok_or_else(|| AppError::Internal("could not determine a home directory".to_owned()))?;
        let data = dirs.data_dir();
        let paths = Self {
            config_dir: dirs.config_dir().to_path_buf(),
            cache_dir: dirs.cache_dir().to_path_buf(),
            autosave_dir: data.join("autosave"),
            log_dir: data.join("logs"),
        };
        paths.create_all()?;
        Ok(paths)
    }

    /// A layout rooted at `root`, for tests: a lifecycle test must never
    /// touch the developer's own recent list or recovery files.
    pub fn in_directory(root: &Path) -> Result<Self> {
        let paths = Self {
            config_dir: root.join("config"),
            cache_dir: root.join("cache"),
            autosave_dir: root.join("autosave"),
            log_dir: root.join("logs"),
        };
        paths.create_all()?;
        Ok(paths)
    }

    fn create_all(&self) -> Result<()> {
        for dir in [
            &self.config_dir,
            &self.cache_dir,
            &self.autosave_dir,
            &self.log_dir,
        ] {
            std::fs::create_dir_all(dir).doing("make the application folder at", dir.display())?;
        }
        Ok(())
    }

    /// The settings file.
    pub fn settings_file(&self) -> PathBuf {
        self.config_dir.join("settings.json")
    }
}
