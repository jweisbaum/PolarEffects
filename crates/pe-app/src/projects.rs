//! Project lifecycle: New, Open, Save, Save As, Close, and the recent list
//! (spec.md 3.3, 4.2–4.4). Ported from VectorEffects' `projects.rs`.
//!
//! The frontend never sees `pe-core` types. What crosses IPC is a flat
//! summary, which keeps `ts-rs` out of the core crate and the wire format
//! readable. Each Tauri command has a plain twin taking `&AppState`, so the
//! whole lifecycle is tested without a webview.

use std::path::PathBuf;

use pe_core::{Boat, CoreError, Project, io};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::{AppError, Context, Result};
use crate::session::{OpenProject, Session};

/// One source as the source list shows it.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "SourceSummary.ts")]
pub struct SourceSummary {
    /// Stable id.
    pub id: u64,
    /// `"orc"`, `"polar_file"` or `"track"`.
    pub kind: String,
    /// Label.
    pub label: String,
    /// `#rrggbb`.
    pub colour: String,
    /// Whether it is shown and blended.
    pub visible: bool,
    /// Blend weight, 0–2.
    pub weight: f64,
}

/// What the frontend needs to know about the open project.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ProjectSummary.ts")]
pub struct ProjectSummary {
    /// The project's identity; names its recovery snapshot.
    pub id: u64,
    /// Display name.
    pub name: String,
    /// Where it lives on disk; null until first saved.
    pub path: Option<String>,
    /// Whether there are changes not yet written to disk.
    pub dirty: bool,
    /// Bumped on every document change.
    pub revision: u64,
    /// Boat name.
    pub boat_name: String,
    /// Boat notes.
    pub boat_notes: String,
    /// Every source, in list order.
    pub sources: Vec<SourceSummary>,
    /// Whether there is anything to undo.
    pub can_undo: bool,
    /// Whether there is anything to redo.
    pub can_redo: bool,
    /// The history label undo would reverse: an English key to translate.
    pub undo_label: Option<String>,
    /// The history label redo would reapply.
    pub redo_label: Option<String>,
}

impl ProjectSummary {
    /// The summary of an open project.
    pub fn of(open: &OpenProject) -> Self {
        let project = &open.project;
        Self {
            id: project.id.raw(),
            name: project.name.clone(),
            path: open.path.as_ref().map(|p| p.to_string_lossy().into_owned()),
            dirty: open.dirty,
            revision: open.revision,
            boat_name: project.boat.name.clone(),
            boat_notes: project.boat.notes.clone(),
            sources: project
                .sources
                .iter()
                .map(|s| SourceSummary {
                    id: s.id.raw(),
                    kind: s.kind.name().to_owned(),
                    label: s.label.clone(),
                    colour: s.colour.to_string(),
                    visible: s.visible,
                    weight: s.weight,
                })
                .collect(),
            can_undo: open.history.can_undo(),
            can_redo: open.history.can_redo(),
            undo_label: open.history.undo_label().map(str::to_owned),
            redo_label: open.history.redo_label().map(str::to_owned),
        }
    }
}

/// A previously opened project (spec.md 3.1, 4.4).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "RecentProject.ts")]
pub struct RecentProject {
    /// Full path.
    pub path: String,
    /// File stem, for display.
    pub name: String,
    /// Whether the file is still there. A missing one is shown greyed with
    /// "Not found".
    pub exists: bool,
}

/// The optional boat fields of the new-project form (spec.md 4.2).
#[derive(Debug, Clone, Default, PartialEq, Deserialize, TS)]
#[ts(export_to = "BoatInput.ts")]
pub struct BoatInput {
    /// Boat name.
    #[serde(default)]
    pub name: String,
    /// Notes.
    #[serde(default)]
    pub notes: String,
}

fn now_unix_s() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// Drops the recovery snapshot of whatever project is about to be replaced
/// or closed: what the user chose to put down is not offered back.
fn forget_open(state: &AppState, session: &Session) {
    if let Some(open) = &session.open {
        crate::autosave::forget(state, open.project.id.raw());
    }
}

/// Persists the settings after a recent-list change. A failure must not fail
/// the open or save that caused it.
fn persist_recent(state: &AppState, session: &Session) {
    let _ = session.settings.save(&state.paths.settings_file());
}

/// Creates a project and makes it the open one.
#[tauri::command]
pub fn new_project(
    state: tauri::State<'_, AppState>,
    name: String,
    boat: Option<BoatInput>,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    create(&state, name, boat, discard_unsaved)
}

/// [`new_project`] without a Tauri handle.
pub fn create(
    state: &AppState,
    name: String,
    boat: Option<BoatInput>,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadOption {
            field: "Project name",
            value: name,
        });
    }
    let boat = boat.unwrap_or_default();
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        forget_open(state, session);
        let project = Project::new(
            name,
            Boat {
                name: boat.name.trim().to_owned(),
                notes: boat.notes,
            },
            now_unix_s(),
        );
        session.open = Some(OpenProject::created(project));
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

/// Opens a project from disk.
#[tauri::command(async)]
pub fn open_project(
    state: tauri::State<'_, AppState>,
    path: String,
    discard_unsaved: bool,
) -> Result<ProjectSummary> {
    open(&state, path, discard_unsaved)
}

/// Reads a project file, keeping a newer-schema refusal as its own kind so
/// the frontend can say "update PolarEffects" rather than "damaged file".
pub(crate) fn read_project(path: &std::path::Path, doing: &'static str) -> Result<Project> {
    match io::load(path) {
        Ok(project) => Ok(project),
        Err(err @ CoreError::SchemaTooNew { .. }) => Err(AppError::Core(err)),
        Err(err) => Err(err).doing(doing, path.display()),
    }
}

/// [`open_project`] without a Tauri handle.
pub fn open(state: &AppState, path: String, discard_unsaved: bool) -> Result<ProjectSummary> {
    let path = PathBuf::from(path);
    // Asked before reading as well as after: a refusal that was always
    // coming should not cost a large read first. Nothing is replaced until
    // the second asking, under the same lock as the replacing.
    state.with_session(|session| session.refuse_to_discard(discard_unsaved))?;
    let project = read_project(&path, "open the project at")?;
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        forget_open(state, session);
        session.open = Some(OpenProject::loaded(project, path.clone()));
        session.settings.remember(&path);
        persist_recent(state, session);
        Ok(ProjectSummary::of(session.require_open()?))
    })
}

/// Saves the open project to its file.
#[tauri::command(async)]
pub fn save_project(state: tauri::State<'_, AppState>) -> Result<ProjectSummary> {
    save(&state)
}

/// [`save_project`] without a Tauri handle.
pub fn save(state: &AppState) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let path = session
            .require_open()?
            .path
            .clone()
            .ok_or(AppError::ProjectNeverSaved)?;
        session.save_to(path)?;
        persist_recent(state, session);
        let open = session.require_open()?;
        // Saved cleanly: there is nothing to recover (spec.md 4.5).
        crate::autosave::forget(state, open.project.id.raw());
        Ok(ProjectSummary::of(open))
    })
}

/// Saves the open project to a new file.
#[tauri::command(async)]
pub fn save_project_as(state: tauri::State<'_, AppState>, path: String) -> Result<ProjectSummary> {
    save_as(&state, path)
}

/// [`save_project_as`] without a Tauri handle.
pub fn save_as(state: &AppState, path: String) -> Result<ProjectSummary> {
    // Add the extension if the user did not, so the project is found again
    // by the same filter that saved it.
    let path = with_extension(PathBuf::from(path));
    state.with_session(|session| {
        session.save_to(path)?;
        persist_recent(state, session);
        let open = session.require_open()?;
        crate::autosave::forget(state, open.project.id.raw());
        Ok(ProjectSummary::of(open))
    })
}

/// Appends `.wpsproj` unless the name already ends in exactly that, so
/// "Race.v2" becomes "Race.v2.wpsproj" rather than "Race.wpsproj".
pub fn with_extension(path: PathBuf) -> PathBuf {
    if path.extension().is_some_and(|ext| ext == io::EXTENSION) {
        return path;
    }
    let mut name = path.into_os_string();
    name.push(".");
    name.push(io::EXTENSION);
    PathBuf::from(name)
}

/// Closes the open project.
#[tauri::command]
pub fn close_project(state: tauri::State<'_, AppState>, discard_unsaved: bool) -> Result<()> {
    close(&state, discard_unsaved)
}

/// [`close_project`] without a Tauri handle. Guarded like every other path
/// that drops the open project (spec.md 3.3).
pub fn close(state: &AppState, discard_unsaved: bool) -> Result<()> {
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        forget_open(state, session);
        session.open = None;
        Ok(())
    })
}

/// The open project, or null on the start screen.
#[tauri::command]
pub fn project_summary(state: tauri::State<'_, AppState>) -> Result<Option<ProjectSummary>> {
    summary(&state)
}

/// [`project_summary`] without a Tauri handle.
pub fn summary(state: &AppState) -> Result<Option<ProjectSummary>> {
    state.with_session(|session| Ok(session.open.as_ref().map(ProjectSummary::of)))
}

/// The recent list, newest first, with missing files marked.
#[tauri::command]
pub fn recent_projects(state: tauri::State<'_, AppState>) -> Result<Vec<RecentProject>> {
    recent(&state)
}

/// [`recent_projects`] without a Tauri handle.
pub fn recent(state: &AppState) -> Result<Vec<RecentProject>> {
    state.with_session(|session| {
        Ok(session
            .settings
            .recent_projects
            .iter()
            .map(|path| RecentProject {
                path: path.to_string_lossy().into_owned(),
                name: path
                    .file_stem()
                    .map(|s| s.to_string_lossy().into_owned())
                    .unwrap_or_default(),
                exists: path.is_file(),
            })
            .collect())
    })
}

/// Forgets one recent entry (a missing file the user clicked, spec.md 3.1).
#[tauri::command]
pub fn forget_recent_project(
    state: tauri::State<'_, AppState>,
    path: String,
) -> Result<Vec<RecentProject>> {
    forget_recent(&state, path)
}

/// [`forget_recent_project`] without a Tauri handle.
pub fn forget_recent(state: &AppState, path: String) -> Result<Vec<RecentProject>> {
    let path = PathBuf::from(path);
    state.with_session(|session| {
        session.settings.recent_projects.retain(|p| p != &path);
        session.settings.save(&state.paths.settings_file())
    })?;
    recent(state)
}

/// Clears the recent list.
///
/// Unlike the remembering on open, a failed write is **not** swallowed:
/// reaching the settings file is the whole of this operation, and a list that
/// silently comes back at the next launch is worse than an error now.
#[tauri::command]
pub fn clear_recent(state: tauri::State<'_, AppState>) -> Result<Vec<RecentProject>> {
    clear_recent_projects(&state)
}

/// [`clear_recent`] without a Tauri handle.
pub fn clear_recent_projects(state: &AppState) -> Result<Vec<RecentProject>> {
    state.with_session(|session| {
        session.settings.recent_projects.clear();
        session.settings.save(&state.paths.settings_file())
    })?;
    recent(state)
}
