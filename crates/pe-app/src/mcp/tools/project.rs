//! The project group (spec.md 3.7): what is open, and opening, saving and
//! closing it.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use schemars::JsonSchema;
use serde::Deserialize;
use tauri::Manager;

use super::{PolarExplorer, ToolResult, absolute, json, may_write};

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectNewParams {
    /// The project's name.
    pub name: String,
    /// The first boat's name. A project starts with one boat.
    #[serde(default)]
    pub boat_name: Option<String>,
    /// Free notes about that boat.
    #[serde(default)]
    pub boat_notes: Option<String>,
    /// Discard unsaved changes in the open project. Refused without it.
    /// Pass true only when the user said to discard them.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectOpenParams {
    /// Path to a .wpsproj file: absolute, or beginning with `~/`.
    pub path: String,
    /// Discard unsaved changes in the open project. Refused without it.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct ProjectSaveParams {
    /// Save here instead of the project's own path (a Save As). Absolute, or
    /// beginning with `~/`; `.wpsproj` is added when missing.
    #[serde(default)]
    pub path: Option<String>,
    /// Replace a file that is already at `path`. Refused without it, unless
    /// that file is the project's own. Pass true only when the user means
    /// the file to be replaced.
    #[serde(default)]
    pub overwrite: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
pub struct DiscardParams {
    /// Discard unsaved changes. Refused without it when there are any.
    #[serde(default)]
    pub discard_unsaved: bool,
}

#[tool_router(router = tool_router_project, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "What PolarExplorer — the sailing-polar application open on this computer — has open: `project` is its summary (name, path, dirty, revision, the first boat's sources with ids, weights and counts, the blend's settings and coverage, undo state), or null when none is open. A good first call after polarexplorer_guide: an open project that is dirty holds the user's unsaved work."
    )]
    async fn project_status(&self) -> ToolResult {
        let project = self
            .run("project_status", |app| {
                crate::projects::project_summary(app.state(), None)
            })
            .await?;
        json(&serde_json::json!({ "project": project }))
    }

    #[tool(
        description = "Starts a new PolarExplorer project with one boat and opens it in the interface: the first step of building a polar. Refused while the open project has unsaved changes unless discard_unsaved is true."
    )]
    async fn project_new(&self, Parameters(p): Parameters<ProjectNewParams>) -> ToolResult {
        let boat =
            (p.boat_name.is_some() || p.boat_notes.is_some()).then(|| crate::projects::BoatInput {
                name: p.boat_name.unwrap_or_default(),
                notes: p.boat_notes.unwrap_or_default(),
            });
        let summary = self
            .write("project_new", true, move |app| {
                crate::projects::new_project(app.state(), p.name, boat, p.discard_unsaved)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Opens a .wpsproj project file in the interface. Refused while the open project has unsaved changes unless discard_unsaved is true."
    )]
    async fn project_open(&self, Parameters(p): Parameters<ProjectOpenParams>) -> ToolResult {
        let path = absolute(&p.path)?;
        let summary = self
            .write("project_open", true, move |app| {
                crate::projects::open_project(app.state(), path, p.discard_unsaved)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Saves the project to its own path, or to `path` as a Save As (absolute or beginning with ~/). A project that has never been saved needs `path`. A file already at `path` is replaced only with overwrite: true. Saving is not needed before an export."
    )]
    async fn project_save(&self, Parameters(p): Parameters<ProjectSaveParams>) -> ToolResult {
        let path = p.path.as_deref().map(absolute).transpose()?;
        if let Some(path) = &path {
            // The file the save would write: the command adds the extension.
            let target = crate::projects::with_extension(std::path::PathBuf::from(path));
            let own = self
                .run("project_save", |app| {
                    crate::projects::project_summary(app.state(), None)
                })
                .await?
                .and_then(|project| project.path)
                .is_some_and(|own| std::path::Path::new(&own) == target);
            if !own {
                may_write(&target, p.overwrite)?;
            }
        }
        let summary = self
            .write("project_save", false, move |app| match path {
                Some(path) => crate::projects::save_project_as(app.state(), path),
                None => crate::projects::save_project(app.state()),
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Closes the project and returns the interface to the start screen. Refused with unsaved changes unless discard_unsaved is true."
    )]
    async fn project_close(&self, Parameters(p): Parameters<DiscardParams>) -> ToolResult {
        self.write("project_close", true, move |app| {
            crate::projects::close_project(app.state(), p.discard_unsaved)
        })
        .await?;
        json(&serde_json::json!({ "closed": true }))
    }

    #[tool(
        description = "Recently opened projects, newest first: each one's path, name and whether the file is still there."
    )]
    async fn recent_projects(&self) -> ToolResult {
        let recent = self
            .run("recent_projects", |app| {
                crate::projects::recent_projects(app.state())
            })
            .await?;
        json(&recent)
    }
}
