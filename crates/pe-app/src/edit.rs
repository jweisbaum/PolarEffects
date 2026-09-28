//! Document edits and undo/redo over IPC (spec.md 4.6).
//!
//! Every change goes through a `pe_core::Command` on the open project's
//! history, which bumps the revision and sets `dirty` (spec.md 3.3). Later
//! milestones add their edits here.

use pe_core::Command;

use crate::commands::AppState;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// Applies the command `build` makes from the open project, and returns the
/// new summary. `build` returns `None` for a change that changes nothing,
/// which records no history entry and leaves the project clean.
pub fn apply(
    state: &AppState,
    build: impl FnOnce(&pe_core::Project) -> Result<Option<Command>>,
) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        if let Some(command) = build(&open.project)? {
            open.apply(command)?;
        }
        Ok(ProjectSummary::of(open))
    })
}

/// Reverses the most recent change.
#[tauri::command]
pub fn undo(state: tauri::State<'_, AppState>) -> Result<ProjectSummary> {
    undo_last(&state)
}

/// [`undo`] without a Tauri handle.
pub fn undo_last(state: &AppState) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        if open.history.undo(&mut open.project)?.is_some() {
            open.touch();
        }
        Ok(ProjectSummary::of(open))
    })
}

/// Reapplies the most recently undone change.
#[tauri::command]
pub fn redo(state: tauri::State<'_, AppState>) -> Result<ProjectSummary> {
    redo_next(&state)
}

/// [`redo`] without a Tauri handle.
pub fn redo_next(state: &AppState) -> Result<ProjectSummary> {
    state.with_session(|session| {
        let open = session.require_open()?;
        if open.history.redo(&mut open.project)?.is_some() {
            open.touch();
        }
        Ok(ProjectSummary::of(open))
    })
}

/// Renames the open project (spec.md 3.2: click the name to rename).
#[tauri::command]
pub fn rename_project(state: tauri::State<'_, AppState>, name: String) -> Result<ProjectSummary> {
    rename(&state, name)
}

/// [`rename_project`] without a Tauri handle.
pub fn rename(state: &AppState, name: String) -> Result<ProjectSummary> {
    let name = name.trim().to_owned();
    if name.is_empty() {
        return Err(AppError::BadOption {
            field: "Project name",
            value: name,
        });
    }
    apply(state, |project| {
        Ok((project.name != name).then(|| Command::RenameProject {
            before: project.name.clone(),
            after: name,
        }))
    })
}
