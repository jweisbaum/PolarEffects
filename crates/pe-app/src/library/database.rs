//! The SYRF database's read-only metadata download (asked 2026-10-06): the
//! boat, race and track rows of the supported trackers, read from the
//! person's PostgreSQL server into `boat-metadata.json` when they press the
//! button in Settings, and merged with what scraping saved there.
//!
//! Nothing here writes to the database. The session is opened with
//! `default_transaction_read_only` on and the snapshot is one `READ ONLY,
//! REPEATABLE READ` transaction, so even a mistaken statement is refused by
//! the server rather than trusted to this code. That is also why the subsets
//! are common table expressions and not the temporary tables the earlier
//! download used: a read-only transaction may not create them.

use super::LibrarySettings;
use super::catalogue::{self, Metadata};
use crate::{
    commands::AppState,
    error::{AppError, Context, Result},
};
use postgres::{Client, config::SslMode};
use rustls_platform_verifier::BuilderVerifierExt;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use tauri::{Emitter, Manager};
use ts_rs::TS;

/// The event a download's progress is sent on.
pub const PROGRESS: &str = "library://metadata";

/// The trackers whose rows are downloaded: the library's own sources, by the
/// database's `source` folded to upper-case letters and digits.
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

/// The tables the download reads, each limited to the supported trackers'
/// races and boats. `$1` is [`SOURCES`].
const SUBSETS: &str = r#"WITH
  ev AS (SELECT id FROM public."CalendarEvents"
         WHERE upper(regexp_replace(source, '[^a-zA-Z0-9]', '', 'g')) = ANY($1)),
  un AS (SELECT c.id, c."vesselParticipantGroupId", c."courseId" FROM public."CompetitionUnits" c
         WHERE c."calendarEventId" IN (SELECT id FROM ev)),
  ve AS (SELECT id FROM public."Vessels"
         WHERE upper(regexp_replace(source, '[^a-zA-Z0-9]', '', 'g')) = ANY($1) AND "deletedAt" IS NULL),
  pa AS (SELECT p.id FROM public."VesselParticipants" p WHERE p."vesselId" IN (SELECT id FROM ve)),
  co AS (SELECT id FROM public."Courses"
         WHERE id IN (SELECT "courseId" FROM un) OR "calendarEventId" IN (SELECT id FROM ev))"#;

/// Each table and which of its rows belong to the subset.
const TABLES: &[(&str, &str)] = &[
    ("Vessels", "r.id IN (SELECT id FROM ve)"),
    ("VesselParticipants", "r.id IN (SELECT id FROM pa)"),
    ("CalendarEvents", "r.id IN (SELECT id FROM ev)"),
    ("CompetitionUnits", "r.id IN (SELECT id FROM un)"),
    (
        "VesselParticipantGroups",
        r#"r.id IN (SELECT "vesselParticipantGroupId" FROM un)"#,
    ),
    (
        "VesselParticipantEvents",
        r#"r."competitionUnitId" IN (SELECT id FROM un) AND r."vesselParticipantId" IN (SELECT id FROM pa)"#,
    ),
    (
        "VesselParticipantTrackJsons",
        r#"r."competitionUnitId" IN (SELECT id FROM un) AND r."vesselParticipantId" IN (SELECT id FROM pa)"#,
    ),
    ("Courses", "r.id IN (SELECT id FROM co)"),
    (
        "CourseUnsequencedUntimedGeometries",
        r#"r."courseId" IN (SELECT id FROM co)"#,
    ),
];

/// Where the SYRF database is. The password stays in the local settings
/// file; it never reaches a log, the metadata or a project.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default)]
pub struct DatabaseConnection {
    /// The server's host name or address.
    pub host: String,
    /// The server's port.
    pub port: u16,
    /// The database's name.
    pub name: String,
    /// The user to sign in as; a read-only role is enough.
    pub user: String,
    /// That user's password; may be empty for local trust sign-in.
    pub password: String,
    /// Require TLS and verify the server's certificate.
    pub tls: bool,
}

impl std::fmt::Debug for DatabaseConnection {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DatabaseConnection")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("name", &self.name)
            .field("user", &self.user)
            .field("tls", &self.tls)
            .finish_non_exhaustive()
    }
}

impl Default for DatabaseConnection {
    fn default() -> Self {
        Self {
            host: "localhost".into(),
            port: 5432,
            name: "syrfbackendprod".into(),
            user: "postgres".into(),
            password: String::new(),
            tls: false,
        }
    }
}

impl DatabaseConnection {
    /// Refuses a connection that names no server, database or user.
    ///
    /// # Errors
    /// [`AppError::Internal`] naming what is missing.
    pub fn validate(&self) -> Result<()> {
        if self.host.trim().is_empty()
            || self.name.trim().is_empty()
            || self.user.trim().is_empty()
            || self.port == 0
        {
            return Err(AppError::Internal(
                "PostgreSQL host, database, user and a valid port are required".into(),
            ));
        }
        Ok(())
    }
}

/// A session the server itself holds read-only.
pub(crate) fn connect(connection: &DatabaseConnection) -> Result<Client> {
    connection.validate()?;
    let mut config = postgres::Config::new();
    config
        .host(&connection.host)
        .port(connection.port)
        .dbname(&connection.name)
        .user(&connection.user)
        .password(&connection.password)
        .connect_timeout(Duration::from_secs(10))
        .application_name("PolarExplorer")
        .options(
            "-c default_transaction_read_only=on -c statement_timeout=300000 -c lock_timeout=10000",
        );
    let result = if connection.tls {
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

/// Connects, checks the session is read-only and every table the download
/// reads is there, and answers the database's name. Reads no rows.
#[tauri::command]
pub async fn test_database_connection(connection: DatabaseConnection) -> Result<String> {
    tauri::async_runtime::spawn_blocking(move || test_connection(&connection))
        .await
        .map_err(|e| AppError::Internal(format!("Connection test worker failed: {e}")))?
}

/// [`test_database_connection`], for callers without Tauri.
///
/// # Errors
/// No connection, a session the server does not hold read-only, or a
/// missing table.
pub fn test_connection(connection: &DatabaseConnection) -> Result<String> {
    let mut client = connect(connection)?;
    let read_only: String = client
        .query_one("SHOW transaction_read_only", &[])
        .doing("check", "the read-only session")?
        .get(0);
    if read_only != "on" {
        return Err(AppError::Internal(
            "The PostgreSQL session is not read-only".into(),
        ));
    }
    for (table, _) in TABLES {
        client
            .simple_query(&format!("SELECT 1 FROM public.\"{table}\" LIMIT 0"))
            .doing("read database table", table)?;
    }
    Ok(client
        .query_one("SELECT current_database()", &[])
        .doing("test", "PostgreSQL")?
        .get(0))
}

/// What a download is doing, or did last.
#[derive(Debug, Clone, Default, Serialize, Deserialize, TS)]
pub struct MetadataProgress {
    /// Whether one is running.
    pub running: bool,
    /// Tables read.
    pub done: u32,
    /// Tables to read.
    pub total: u32,
    /// The table it is reading now.
    pub current: String,
    /// Searchable tracks the database gave, once it finished.
    pub tracks: u32,
    /// Tracks scraping saved that stay beside them.
    pub kept: u32,
    /// Whether it was cancelled.
    pub cancelled: bool,
    /// What stopped it, when something did.
    pub error: Option<String>,
}

#[derive(Debug, Default)]
struct Store {
    progress: MetadataProgress,
    cancel: Option<Arc<AtomicBool>>,
}

/// The download's state in the application.
#[derive(Debug, Default)]
pub struct DownloadState {
    store: Mutex<Store>,
}

impl DownloadState {
    fn lock(&self) -> Result<std::sync::MutexGuard<'_, Store>> {
        self.store
            .lock()
            .map_err(|_| AppError::Internal("The metadata download's lock was poisoned".into()))
    }

    pub(crate) fn running(&self) -> bool {
        self.lock().is_ok_and(|s| s.progress.running)
    }
}

fn update<R: tauri::Runtime>(app: &tauri::AppHandle<R>, f: impl FnOnce(&mut MetadataProgress)) {
    let state = app.state::<AppState>();
    let progress = match state.library.download.lock() {
        Ok(mut s) => {
            f(&mut s.progress);
            s.progress.clone()
        }
        Err(_) => return,
    };
    let _ = app.emit(PROGRESS, progress);
}

/// The download's progress.
#[tauri::command]
pub fn metadata_download_status(state: tauri::State<'_, AppState>) -> Result<MetadataProgress> {
    Ok(state.library.download.lock()?.progress.clone())
}

/// Asks the running download to stop; the metadata file is left as it was.
#[tauri::command]
pub fn cancel_metadata_download(state: tauri::State<'_, AppState>) -> Result<()> {
    if let Some(c) = &state.library.download.lock()?.cancel {
        c.store(true, Ordering::SeqCst);
    }
    Ok(())
}

/// Starts a download with the library settings as saved.
#[tauri::command]
pub fn start_metadata_download(app: tauri::AppHandle) -> Result<MetadataProgress> {
    start(app)
}

fn start<R: tauri::Runtime>(app: tauri::AppHandle<R>) -> Result<MetadataProgress> {
    let state = app.state::<AppState>();
    let settings = super::settings(&state)?;
    settings.validate()?;
    if state.library.scrape.running() {
        return Err(AppError::Internal(
            "Wait for the scrape to finish before downloading metadata".into(),
        ));
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let progress = MetadataProgress {
        running: true,
        total: TABLES.len() as u32,
        ..Default::default()
    };
    {
        let mut store = state.library.download.lock()?;
        if store.progress.running {
            return Err(AppError::Internal(
                "A metadata download is already running".into(),
            ));
        }
        store.progress = progress.clone();
        store.cancel = Some(Arc::clone(&cancel));
    }
    let _ = app.emit(PROGRESS, progress.clone());
    tauri::async_runtime::spawn_blocking(move || {
        let state = app.state::<AppState>();
        let result = download(&state, &settings, &cancel, |done, table| {
            update(&app, |p| {
                p.done = done;
                p.current = table.to_owned();
            });
        });
        update(&app, |p| {
            p.running = false;
            p.current = String::new();
            match &result {
                Ok((tracks, kept)) => {
                    p.done = p.total;
                    p.tracks = *tracks as u32;
                    p.kept = *kept as u32;
                }
                Err(_) if cancel.load(Ordering::SeqCst) => p.cancelled = true,
                Err(e) => p.error = Some(e.to_string()),
            }
        });
        if let Ok(mut store) = state.library.download.lock() {
            store.cancel = None;
        }
    });
    Ok(progress)
}

fn check(cancel: &AtomicBool) -> Result<()> {
    if cancel.load(Ordering::SeqCst) {
        Err(AppError::Tracker(pe_trackers::TrackerError::Cancelled))
    } else {
        Ok(())
    }
}

/// Reads the subset in one read-only snapshot, merges it into the metadata
/// file and writes that once, at the end. Answers the database's searchable
/// tracks and the scraped tracks kept beside them.
///
/// # Errors
/// No connection, a refused query, cancellation (the file is untouched), or
/// the file not being read or written.
pub(crate) fn download(
    state: &AppState,
    settings: &LibrarySettings,
    cancel: &AtomicBool,
    mut progress: impl FnMut(u32, &str),
) -> Result<(usize, usize)> {
    check(cancel)?;
    // Read the file first: one this build cannot read is refused before the
    // database is asked for anything.
    let existing = catalogue::read_metadata(state, settings)?;
    let mut client = connect(&settings.database)?;
    let mut tx = client
        .build_transaction()
        .isolation_level(postgres::IsolationLevel::RepeatableRead)
        .read_only(true)
        .start()
        .doing("snapshot", "boat metadata")?;
    let mut tables = BTreeMap::new();
    for (i, (table, filter)) in TABLES.iter().enumerate() {
        check(cancel)?;
        progress(i as u32, table);
        let rows = tx
            .query(
                &format!(
                    "{SUBSETS} SELECT row_to_json(r) FROM public.\"{table}\" r WHERE {filter} ORDER BY r.id"
                ),
                &[&SOURCES],
            )
            .doing("download metadata from", table)?;
        tables.insert(
            (*table).to_owned(),
            rows.into_iter().map(|r| r.get::<_, Value>(0)).collect(),
        );
    }
    // Nothing was written; ending the snapshot either way changes nothing.
    tx.rollback().doing("finish", "metadata snapshot")?;
    check(cancel)?;
    let mut merged = merge(existing, tables);
    if !settings.geojson_directory.is_empty() {
        super::scrape::drop_duplicates(&mut merged, Path::new(&settings.geojson_directory));
    }
    let scraped = scraped(&merged);
    let kept = merged
        .tracks
        .iter()
        .filter(|t| scraped.units.contains(&t.competition_id))
        .count();
    let tracks = merged.tracks.len() - kept;
    check(cancel)?;
    catalogue::write_metadata(state, settings, &merged)?;
    Ok((tracks, kept))
}

/// The records scraping saved, told apart by their ids: the scraper derives
/// each race's event id from its source and catalogue key, which no
/// database row's id is.
struct Scraped {
    events: BTreeSet<String>,
    units: BTreeSet<String>,
    vessels: BTreeSet<String>,
}

fn scraped(metadata: &Metadata) -> Scraped {
    let events: BTreeSet<String> = metadata
        .tables
        .get("CalendarEvents")
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|e| {
            let id = catalogue::text(e, "id");
            id == super::scrape::event_id(
                &catalogue::text(e, "source"),
                &catalogue::text(e, "scrapedOriginalId"),
            )
        })
        .map(|e| catalogue::text(e, "id"))
        .collect();
    let units: BTreeSet<String> = metadata
        .tables
        .get("CompetitionUnits")
        .map_or(&[][..], Vec::as_slice)
        .iter()
        .filter(|u| events.contains(&catalogue::text(u, "calendarEventId")))
        .map(|u| catalogue::text(u, "id"))
        .collect();
    let vessels = metadata
        .tracks
        .iter()
        .filter(|t| units.contains(&t.competition_id))
        .map(|t| t.vessel_id.clone())
        .collect();
    Scraped {
        events,
        units,
        vessels,
    }
}

/// The database's rows and search records, replacing the database's earlier
/// ones, with what scraping saved kept beside them (asked 2026-10-06).
pub(super) fn merge(existing: Metadata, tables: BTreeMap<String, Vec<Value>>) -> Metadata {
    let mine = scraped(&existing);
    let mut tracks = catalogue::index(&tables);
    let mut tables = tables;
    for (table, ids) in [
        ("CalendarEvents", &mine.events),
        ("CompetitionUnits", &mine.units),
        ("Vessels", &mine.vessels),
    ] {
        let Some(rows) = existing.tables.get(table) else {
            continue;
        };
        let target = tables.entry(table.to_owned()).or_default();
        let present: BTreeSet<String> = target.iter().map(|r| catalogue::text(r, "id")).collect();
        target.extend(
            rows.iter()
                .filter(|r| {
                    let id = catalogue::text(r, "id");
                    ids.contains(&id) && !present.contains(&id)
                })
                .cloned(),
        );
    }
    tracks.extend(
        existing
            .tracks
            .into_iter()
            .filter(|t| mine.units.contains(&t.competition_id)),
    );
    Metadata {
        version: 1,
        tables,
        tracks,
    }
}
