//! Shared by the integration tests: a throwaway application root.
#![allow(
    dead_code,
    clippy::expect_used,
    reason = "each test binary uses a different part"
)]

use std::path::PathBuf;

use pe_app::commands::AppState;
use pe_app::paths::AppPaths;

/// A temporary directory that is removed on drop.
#[derive(Debug)]
pub struct TempRoot(pub PathBuf);

impl TempRoot {
    pub fn new(label: &str) -> Self {
        let dir = std::env::temp_dir().join(format!(
            "pe-app-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        std::fs::create_dir_all(&dir).expect("temp root");
        Self(dir)
    }

    pub fn file(&self, name: &str) -> String {
        self.0.join(name).to_string_lossy().into_owned()
    }

    pub fn state(&self) -> AppState {
        AppState::new(AppPaths::in_directory(&self.0.join("app")).expect("paths"))
    }
}

impl Drop for TempRoot {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
