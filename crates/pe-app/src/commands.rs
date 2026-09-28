//! The IPC command surface.
//!
//! Every function here is a Tauri command. Each returns
//! [`crate::error::Result`], so a failure always reaches the frontend as
//! `{ kind, message }`, and each type it returns is exported to TypeScript by
//! `examples/export_bindings.rs`.

use serde::Serialize;
use ts_rs::TS;

use crate::error::{AppError, Result};
use crate::paths::AppPaths;
use crate::session::Session;

/// Everything the running application holds, managed by Tauri.
#[derive(Debug)]
pub struct AppState {
    /// Resolved application directories.
    pub paths: AppPaths,
    /// The open project and the settings.
    pub session: std::sync::Mutex<Session>,
}

impl AppState {
    /// State over `paths`, with the settings read from disk.
    pub fn new(paths: AppPaths) -> Self {
        let session = Session::load(&paths.settings_file());
        Self {
            paths,
            session: std::sync::Mutex::new(session),
        }
    }

    /// Runs `f` with the session locked.
    pub fn with_session<T>(&self, f: impl FnOnce(&mut Session) -> Result<T>) -> Result<T> {
        let mut session = self
            .session
            .lock()
            .map_err(|_| AppError::Internal("the session lock was poisoned".to_owned()))?;
        f(&mut session)
    }
}

/// Build facts for the start screen and the About panel.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, TS)]
#[ts(export_to = "AppInfo.ts")]
pub struct AppInfo {
    /// Product name.
    pub name: String,
    /// Semantic version of this build.
    pub version: String,
}

/// The application's name and version.
#[tauri::command]
pub fn app_info() -> Result<AppInfo> {
    Ok(AppInfo {
        name: "PolarEffects".to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The version the frontend shows must be the one the bundle is stamped
    /// with, which Tauri takes from its own config, not from Cargo.
    #[test]
    fn app_info_matches_the_bundle_config() {
        let conf: serde_json::Value =
            serde_json::from_str(include_str!("../tauri.conf.json")).unwrap();
        let info = app_info().unwrap();
        assert_eq!(info.name, conf["productName"]);
        assert_eq!(info.version, conf["version"]);
    }

    #[test]
    fn app_info_crosses_ipc_as_name_and_version() {
        let json = serde_json::to_value(app_info().unwrap()).unwrap();
        assert_eq!(
            json,
            serde_json::json!({ "name": "PolarEffects", "version": env!("CARGO_PKG_VERSION") })
        );
    }
}
