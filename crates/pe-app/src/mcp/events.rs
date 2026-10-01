//! What the service tells the frontend, so the interface follows the client
//! (spec.md 3.7).
//!
//! **Emitted by the service only.** A command the interface invoked returns
//! its summary to the caller; emitting here as well would refresh every
//! panel twice per edit.

use serde::Serialize;
use tauri::{Emitter, Manager};
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::Result;
use crate::projects::ProjectSummary;

/// The document changed, or a project was opened, saved or closed.
pub const CHANGED: &str = "document://changed";
/// Show a stage: the 3D view, the map, Compare, or the full-size plot.
pub const STAGE: &str = "view://stage";
/// Show a boat's tab.
pub const BOAT: &str = "view://boat";
/// Select these samples, as a selection made on the map would.
pub const SELECTION: &str = "view://selection";
/// A client session opened or closed, or a tool was called.
pub const ACTIVITY: &str = "mcp://activity";

/// The document after a tool wrote to it. `project` null means closed.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "DocumentChanged.ts")]
pub struct DocumentChanged {
    /// The open project's summary, as the interface's own reads give it.
    pub project: Option<ProjectSummary>,
    /// A different project than before: the frontend resets its stage,
    /// selection and boat tab, as its own open path does.
    pub opened: bool,
}

// The screenshot round trip is its own module, but its event belongs in the
// list of what the service tells the frontend.
pub use super::capture::{CAPTURE, CaptureRequest};

/// The stage to show.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "ViewStage.ts")]
pub struct ViewStage {
    /// `"3d"`, `"map"`, `"compare"` or `"plot"` (the full-size polar plot).
    pub stage: String,
    /// The boat whose stage it is, and whose tab to show: each boat's view
    /// has a stage of its own.
    pub boat: u64,
}

/// The boat whose tab to show.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "ViewBoat.ts")]
pub struct ViewBoat {
    /// The boat's id.
    pub boat: u64,
}

/// The samples to select, as a selection made on the map would.
#[derive(Debug, Clone, Serialize, TS)]
#[ts(export_to = "ViewSelection.ts")]
pub struct ViewSelection {
    /// The boat they belong to; null for the boat on show.
    pub boat: Option<u64>,
    /// Sample ids; empty clears the selection.
    pub samples: Vec<u64>,
}

/// What a client is doing, for the status bar.
#[derive(Debug, Clone, Default, Serialize, TS)]
#[ts(export_to = "McpActivity.ts")]
pub struct McpActivity {
    /// Open client sessions.
    pub sessions: u32,
    /// The last tool a client called, while a session is open.
    pub last_tool: Option<String>,
}

/// Reads the summary and tells the frontend.
///
/// Generic over the Tauri runtime so the mock application used by the
/// integration tests can drive the same path as the shipped build.
pub fn changed<R: tauri::Runtime>(
    app: &tauri::AppHandle<R>,
    opened: bool,
) -> Result<Option<ProjectSummary>> {
    let project = crate::projects::summary(app.state::<AppState>().inner())?;
    let _ = app.emit(
        CHANGED,
        DocumentChanged {
            project: project.clone(),
            opened,
        },
    );
    Ok(project)
}
