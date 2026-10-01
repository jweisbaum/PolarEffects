//! User-configured PostgreSQL and local SYRF track library. HTTP remains in pe-trackers.
pub mod catalogue;
mod export;
mod ingest;
pub mod jobs;

pub use catalogue::{BoatTrackHit, BoatTrackSearch, import_database_track, search_database_boats};
pub use jobs::{
    DatabaseProgress, DatabaseState, cancel_database_job, database_job_status, start_database_job,
};
pub use jobs::{on_startup, shutdown};

use crate::{
    commands::AppState,
    error::{AppError, Context, Result},
    settings::Settings,
};
use postgres::{Client, config::SslMode};
use rustls_platform_verifier::BuilderVerifierExt;
use serde::{Deserialize, Serialize};
use std::{path::PathBuf, sync::Arc, time::Duration};
use ts_rs::TS;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "snake_case")]
pub enum ScrapeSchedule {
    #[default]
    OnDemand,
    Startup,
    Shutdown,
}

/// Passwords never enter project files, catalogues, job logs or database exports.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct DatabaseSettings {
    pub host: String,
    pub port: u16,
    pub name: String,
    pub user: String,
    pub password: String,
    pub tls: bool,
    pub geojson_directory: String,
    pub metadata_directory: String,
    pub scrape_schedule: ScrapeSchedule,
    /// Optional explicit race URLs, one per line. Empty discovers public races.
    pub scrape_urls: String,
    /// Local-only mobile API credentials; never included in metadata or exports.
    pub yellowbrick_user_key: String,
    pub yellowbrick_device_id: String,
    /// Optional pg_dump executable. Export only; scraping never uses executables.
    pub pg_dump: String,
}
impl std::fmt::Debug for DatabaseSettings {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseSettings")
            .field("host", &self.host)
            .field("name", &self.name)
            .finish_non_exhaustive()
    }
}
impl Default for DatabaseSettings {
    fn default() -> Self {
        let local = PathBuf::from("/Volumes/Disk_Three/s3/syrf-tracks-individual-production");
        Self {
            host: "localhost".into(),
            port: 5432,
            name: "syrfbackendprod".into(),
            user: "postgres".into(),
            password: String::new(),
            tls: false,
            geojson_directory: if local.is_dir() {
                local.to_string_lossy().into_owned()
            } else {
                String::new()
            },
            metadata_directory: String::new(),
            scrape_schedule: ScrapeSchedule::OnDemand,
            scrape_urls: String::new(),
            yellowbrick_user_key: String::new(),
            yellowbrick_device_id: String::new(),
            pg_dump: String::new(),
        }
    }
}
impl DatabaseSettings {
    pub fn validate(&self) -> Result<()> {
        if self.yellowbrick_user_key.trim().is_empty()
            != self.yellowbrick_device_id.trim().is_empty()
        {
            return Err(AppError::Internal(
                "Enter both the YellowBrick user key and device ID, or leave both empty".into(),
            ));
        }
        if self.host.trim().is_empty()
            || self.name.trim().is_empty()
            || self.user.trim().is_empty()
            || self.port == 0
        {
            return Err(AppError::Internal(
                "PostgreSQL host, database, user and a valid port are required".into(),
            ));
        }
        for path in [&self.geojson_directory, &self.metadata_directory] {
            if !path.is_empty() && !std::path::Path::new(path).is_absolute() {
                return Err(AppError::Internal(
                    "Choose an absolute directory path".into(),
                ));
            }
        }
        Ok(())
    }
    pub fn metadata_path(&self, state: &AppState) -> PathBuf {
        if self.metadata_directory.is_empty() {
            state.paths.config_dir.join("boat-metadata")
        } else {
            PathBuf::from(&self.metadata_directory)
        }
        .join("boat-metadata.json")
    }
}

pub(super) fn connect(settings: &DatabaseSettings) -> Result<Client> {
    settings.validate()?;
    let mut config = postgres::Config::new();
    config
        .host(&settings.host)
        .port(settings.port)
        .dbname(&settings.name)
        .user(&settings.user)
        .password(&settings.password)
        .connect_timeout(Duration::from_secs(10))
        .application_name("PolarEffects")
        .options("-c statement_timeout=120000 -c lock_timeout=10000");
    let result = if settings.tls {
        config.ssl_mode(SslMode::Require);
        let tls = rustls::ClientConfig::builder_with_provider(Arc::new(
            rustls::crypto::ring::default_provider(),
        ))
        .with_safe_default_protocol_versions()
        .doing("configure TLS for", "PostgreSQL")?
        .with_platform_verifier()
        .doing("configure certificates for", "PostgreSQL")?
        .with_no_client_auth();
        config.connect(tokio_postgres_rustls::MakeRustlsConnect::new(tls))
    } else {
        config.ssl_mode(SslMode::Disable);
        config.connect(postgres::NoTls)
    };
    result.map_err(|e| AppError::Internal(format!("PostgreSQL connection failed: {e}")))
}

#[tauri::command]
pub async fn test_database_connection(settings: DatabaseSettings) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || test_connection(&settings))
        .await
        .map_err(|e| AppError::Internal(format!("Connection test worker failed: {e}")))?
}
pub fn test_connection(settings: &DatabaseSettings) -> Result<String> {
    let mut client = connect(settings)?;
    for table in [
        "Vessels",
        "VesselParticipants",
        "VesselParticipantEvents",
        "CalendarEvents",
        "CompetitionUnits",
        "Courses",
        "CourseUnsequencedUntimedGeometries",
        "VesselParticipantGroups",
        "VesselParticipantTrackJsons",
    ] {
        client
            .simple_query(&format!("SELECT 1 FROM public.\"{table}\" LIMIT 0"))
            .doing("read database table", table)?;
    }
    Ok(client
        .query_one("SELECT current_database()", &[])
        .doing("test", "PostgreSQL")?
        .get(0))
}

#[tauri::command]
pub fn set_database_settings(
    state: tauri::State<'_, AppState>,
    settings: DatabaseSettings,
) -> Result<Settings> {
    settings.validate()?;
    let store = state.database.lock()?;
    if store.progress.running {
        return Err(AppError::Internal(
            "Wait for the database operation to finish before changing its settings".into(),
        ));
    }
    state.with_session(|session| {
        let mut next = session.settings.clone();
        next.database = settings;
        next.save(&state.paths.settings_file())?;
        session.settings = next.clone();
        Ok(next)
    })
}

pub(super) fn settings(state: &AppState) -> Result<DatabaseSettings> {
    state.with_session(|s| Ok(s.settings.database.clone()))
}
pub(super) fn check(cancel: &std::sync::atomic::AtomicBool) -> Result<()> {
    if cancel.load(std::sync::atomic::Ordering::SeqCst) {
        Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled))
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests;
