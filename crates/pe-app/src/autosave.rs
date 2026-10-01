//! Crash recovery (spec.md 4.5). Ported from VectorEffects' `autosave.rs`.
//!
//! A thread looks at the open project every [`TICK`]. If it is dirty, has
//! changed since the last snapshot, and either [`INTERVAL`] has passed or
//! [`ENTRIES`] history entries have been recorded since — whichever comes
//! first — the document is cloned out from under the session lock and saved
//! to `<data_dir>/autosave/<project id>.wpsproj`, beside a small manifest
//! naming where the project came from. A clean save or a deliberate close
//! removes the snapshot; a crash leaves it, and the start screen offers it
//! back under Recovered work.
//!
//! **Cloned, then saved, never saved under the lock.** A project with many
//! tracks takes long enough to write that holding the session for it would
//! stall every edit.
//!
//! **A snapshot is never a source.** Recovering one opens it *as the original
//! project*, dirty, at its original path if it had one, so the next Save
//! writes where the user meant, and the snapshot goes when it does.

use std::path::{Path, PathBuf};
use std::time::Duration;

use pe_core::io;
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};
use crate::projects::ProjectSummary;
use crate::session::OpenProject;
use crate::settings::AutosaveMode;

/// How often the thread looks.
pub const TICK: Duration = Duration::from_secs(5);
/// The longest a dirty project goes between snapshots (spec.md 4.5).
pub const INTERVAL: Duration = Duration::from_secs(60);
/// The most history entries that go by between snapshots (spec.md 4.5).
pub const ENTRIES: u64 = 50;

/// What the manifest beside a snapshot records.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct Manifest {
    name: String,
    original_path: Option<PathBuf>,
    saved_unix_s: u64,
    /// The revision the snapshot was taken at, so an unchanged project is
    /// not written again.
    revision: u64,
    /// History entries recorded when it was taken, for the 50-entry trigger.
    recorded: u64,
}

/// A snapshot the start screen can offer back.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "RecoveredProject.ts")]
pub struct RecoveredProject {
    /// The project id, which is also the snapshot's file stem.
    pub id: u64,
    /// The project's name.
    pub name: String,
    /// Where it lived; null for a project never saved.
    pub original_path: Option<String>,
    /// When the snapshot was taken, UTC epoch seconds.
    pub saved_unix_s: u64,
}

fn snapshot_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.{}", io::EXTENSION))
}

fn manifest_path(dir: &Path, id: u64) -> PathBuf {
    dir.join(format!("{id}.json"))
}

fn now_unix_s() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// Takes a snapshot of the open project if one is due; returns whether one
/// was written.
///
/// Due means: dirty, changed since the last snapshot, and either
/// [`INTERVAL`] has passed or [`ENTRIES`] entries have been recorded since —
/// or there is no snapshot yet. `force` skips the two timers (tests); the
/// dirty-and-changed checks are never skipped.
pub fn snapshot(state: &AppState, force: bool) -> Result<bool> {
    snapshot_with_hook(state, force, || {})
}

/// [`snapshot`], running `between` once the document has been cloned and
/// the lock released, before the files are written. Only tests pass anything
/// but a no-op: it is how they force a Save or a Close into the unlocked gap.
#[doc(hidden)]
pub fn snapshot_with_hook(state: &AppState, force: bool, between: impl FnOnce()) -> Result<bool> {
    let dir = &state.paths.autosave_dir;
    let Some((project, path, revision, recorded, saves, mode)) = state.with_session(|session| {
        // The user already answered the quit guard. "Don't save" forgot the
        // snapshot; writing one now would offer the discarded work back.
        // Read under the lock, which is where `quit::confirm` sets it.
        if exiting(state) {
            return Ok(None);
        }
        let mode = session.settings.autosave;
        Ok(session.open.as_ref().and_then(|open| {
            open.dirty.then(|| {
                (
                    session.document().unwrap_or_else(|_| open.project.clone()),
                    open.path.clone(),
                    open.revision,
                    open.history.recorded()
                        + session
                            .boats
                            .iter()
                            .map(|boat| boat.history.recorded())
                            .sum::<u64>(),
                    open.saves,
                    mode,
                )
            })
        }))
    })?
    else {
        return Ok(false);
    };
    if mode == AutosaveMode::Off {
        return Ok(false);
    }

    let id = project.id.raw();
    let previous = std::fs::read_to_string(manifest_path(dir, id))
        .ok()
        .and_then(|text| serde_json::from_str::<Manifest>(&text).ok());
    if let Some(previous) = &previous {
        if previous.revision == revision {
            return Ok(false);
        }
        let elapsed = now_unix_s().saturating_sub(previous.saved_unix_s);
        let burst = recorded.saturating_sub(previous.recorded) >= ENTRIES;
        if !force && elapsed < INTERVAL.as_secs() && !burst {
            return Ok(false);
        }
    }

    // The session is unlocked from here until the check at the end.
    between();

    std::fs::create_dir_all(dir).doing("make the autosave folder at", dir.display())?;
    let in_place = mode == AutosaveMode::Save && path.is_some();
    if in_place {
        // Written in place: the save clears any snapshot, and the manifest
        // written below only carries the cadence. The exit flag is checked
        // under the same lock as the save, so a "Don't save" answered in the
        // unlocked gap above is never overridden by writing the project.
        let saved = state.with_session(|session| {
            if exiting(state) {
                return Ok(false);
            }
            crate::projects::save_locked(state, session).map(|_| true)
        })?;
        if !saved {
            return Ok(false);
        }
    }
    let manifest = Manifest {
        name: project.name.clone(),
        original_path: path,
        saved_unix_s: now_unix_s(),
        revision,
        recorded,
    };
    let manifest_json = serde_json::to_string_pretty(&manifest).doing(
        "write the autosave record to",
        manifest_path(dir, id).display(),
    )?;
    if in_place {
        let target = manifest_path(dir, id);
        io::write_atomic(&target, manifest_json.as_bytes())
            .doing("write the autosave record to", target.display())?;
        return Ok(true);
    }

    // Written outside the lock to a file nobody reads, then put in place
    // under the lock, after the checks (M3 carry). A Save or a Close in the
    // unlocked gap already called `forget`, and the quit guard's "Don't
    // save" forgets and allows the exit under the lock: in either case the
    // snapshot is stale work that would be offered back as Recovered, so it
    // is dropped instead of renamed. Nothing stale is ever visible, even for
    // an instant, to a crash or to the start screen.
    let writing = dir.join(format!("{id}.writing"));
    if let Err(err) = io::save(&project, &writing) {
        let _ = std::fs::remove_file(&writing);
        return Err(err).doing("write a recovery snapshot to", writing.display());
    }
    state.with_session(|session| {
        let still_current = !exiting(state)
            && session.open.as_ref().is_some_and(|open| {
                open.project.id.raw() == id && open.dirty && open.saves == saves
            });
        if !still_current {
            let _ = std::fs::remove_file(&writing);
            return Ok(false);
        }
        let target = snapshot_path(dir, id);
        if let Err(err) = std::fs::rename(&writing, &target) {
            let _ = std::fs::remove_file(&writing);
            return Err(err).doing("write a recovery snapshot to", target.display());
        }
        let record = manifest_path(dir, id);
        io::write_atomic(&record, manifest_json.as_bytes())
            .doing("write the autosave record to", record.display())?;
        Ok(true)
    })
}

/// Whether the user has answered the quit guard and the process is on its
/// way out.
fn exiting(state: &AppState) -> bool {
    state.exit_allowed.load(std::sync::atomic::Ordering::SeqCst)
}

/// Removes a project's snapshot, if there is one.
pub fn forget(state: &AppState, id: u64) {
    let dir = &state.paths.autosave_dir;
    let _ = std::fs::remove_file(snapshot_path(dir, id));
    let _ = std::fs::remove_file(manifest_path(dir, id));
}

/// Every snapshot on disk, newest first.
pub fn list(state: &AppState) -> Vec<RecoveredProject> {
    let dir = &state.paths.autosave_dir;
    let mut out: Vec<RecoveredProject> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| {
            let path = entry.path();
            if path.extension()? != "json" {
                return None;
            }
            let id: u64 = path.file_stem()?.to_str()?.parse().ok()?;
            if !snapshot_path(dir, id).exists() {
                return None;
            }
            let manifest: Manifest =
                serde_json::from_str(&std::fs::read_to_string(&path).ok()?).ok()?;
            Some(RecoveredProject {
                id,
                name: manifest.name,
                original_path: manifest
                    .original_path
                    .map(|p| p.to_string_lossy().into_owned()),
                saved_unix_s: manifest.saved_unix_s,
            })
        })
        .collect();
    // Newest first; ties by id so the order never depends on the directory.
    out.sort_by_key(|entry| (std::cmp::Reverse(entry.saved_unix_s), entry.id));
    out
}

/// Opens a snapshot as the project it was taken from: dirty, at its
/// original path if it had one.
pub fn recover(state: &AppState, id: u64, discard_unsaved: bool) -> Result<ProjectSummary> {
    let dir = &state.paths.autosave_dir;
    let manifest: Manifest = std::fs::read_to_string(manifest_path(dir, id))
        .ok()
        .and_then(|text| serde_json::from_str(&text).ok())
        .ok_or_else(|| AppError::BadOption {
            field: "Recovered project",
            value: id.to_string(),
        })?;
    state.with_session(|session| session.refuse_to_discard(discard_unsaved))?;
    let project =
        crate::projects::read_project(&snapshot_path(dir, id), "reopen the recovery snapshot at")?;
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        if let Some(open) = &session.open
            && open.project.id.raw() != id
        {
            forget(state, open.project.id.raw());
        }
        let mut open = match manifest.original_path.clone() {
            Some(path) => OpenProject::loaded(project, path),
            None => OpenProject::created(project),
        };
        open.dirty = true;
        session.replace(open);
        state.env_jobs.cancel(None);
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

/// Starts the snapshot thread for the life of the process.
pub fn start(app: tauri::AppHandle) {
    let _ = std::thread::Builder::new()
        .name("autosave".to_owned())
        .spawn(move || {
            use tauri::Manager;
            loop {
                std::thread::sleep(TICK);
                let state = app.state::<AppState>();
                if let Err(err) = snapshot(&state, false) {
                    eprintln!("PolarEffects: crash-recovery snapshot failed: {err}");
                }
            }
        });
}

/// The snapshots on disk, for the start screen's Recovered work.
#[tauri::command]
pub fn recovered_projects(state: tauri::State<'_, AppState>) -> Result<Vec<RecoveredProject>> {
    Ok(list(&state))
}

/// Opens a snapshot as the project it was taken from.
#[tauri::command(async)]
pub fn open_recovered(
    state: tauri::State<'_, AppState>,
    id: u64,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    recover(&state, id, discard_unsaved)
}

/// Drops a snapshot the user does not want back.
#[tauri::command]
pub fn discard_recovered(
    state: tauri::State<'_, AppState>,
    id: u64,
) -> Result<Vec<RecoveredProject>> {
    forget(&state, id);
    Ok(list(&state))
}
