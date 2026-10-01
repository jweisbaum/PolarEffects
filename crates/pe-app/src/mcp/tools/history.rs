//! The history group (spec.md 3.7): undo and redo.

use rmcp::handler::server::wrapper::Parameters;
use rmcp::{tool, tool_router};
use tauri::Manager;

use super::{BoatParams, PolarExplorer, ToolResult, json};

#[tool_router(router = tool_router_history, vis = "pub(crate)")]
impl<R: tauri::Runtime> PolarExplorer<R> {
    #[tool(
        description = "Undoes the last edit to the boat (the summary's `undo_label` names it): yours or the user's, they share one history per boat."
    )]
    async fn undo(&self, Parameters(p): Parameters<BoatParams>) -> ToolResult {
        let summary = self
            .write("undo", false, move |app| {
                crate::edit::undo(app.state(), p.boat)
            })
            .await?;
        json(&summary)
    }

    #[tool(
        description = "Redoes the last undone edit to the boat (the summary's `redo_label` names it)."
    )]
    async fn redo(&self, Parameters(p): Parameters<BoatParams>) -> ToolResult {
        let summary = self
            .write("redo", false, move |app| {
                crate::edit::redo(app.state(), p.boat)
            })
            .await?;
        json(&summary)
    }
}
