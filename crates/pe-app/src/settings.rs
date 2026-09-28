//! Global settings, stored in `settings.json` beside the recent list
//! (spec.md 3.4).
//!
//! Every field has a default and the file **grows**: a field missing from a
//! file written by an older build takes its default, and a file that fails to
//! parse falls back to defaults entirely. Losing the recent list is far
//! better than refusing to start. M2 adds language, theme and units here.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::error::{Context, Result};

/// How many recent projects are remembered (spec.md 4.4).
pub const MAX_RECENT: usize = 10;

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

/// The settings file.
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    /// Most recently opened or saved projects, newest first.
    pub recent_projects: Vec<PathBuf>,
    /// Autosave behaviour.
    pub autosave: AutosaveMode,
}

impl Settings {
    /// Reads the settings, falling back to defaults on any failure.
    pub fn load(file: &Path) -> Self {
        let mut settings: Self = std::fs::read_to_string(file)
            .ok()
            .and_then(|text| serde_json::from_str(&text).ok())
            .unwrap_or_default();
        settings.recent_projects.truncate(MAX_RECENT);
        settings
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
}

#[cfg(test)]
mod tests {
    use super::*;

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

    #[test]
    fn a_broken_or_partial_file_falls_back_to_defaults() {
        let dir = std::env::temp_dir().join(format!("pe-settings-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("settings.json");
        std::fs::write(&file, "{ not json").unwrap();
        assert_eq!(Settings::load(&file), Settings::default());

        std::fs::write(&file, r#"{ "recent_projects": ["/x.wpsproj"] }"#).unwrap();
        let partial = Settings::load(&file);
        assert_eq!(partial.recent_projects, vec![PathBuf::from("/x.wpsproj")]);
        assert_eq!(partial.autosave, AutosaveMode::Recovery);

        let mut s = Settings {
            autosave: AutosaveMode::Off,
            ..Settings::default()
        };
        s.remember(Path::new("/y.wpsproj"));
        s.save(&file).unwrap();
        assert_eq!(Settings::load(&file), s);
        let _ = std::fs::remove_dir_all(&dir);
    }
}
