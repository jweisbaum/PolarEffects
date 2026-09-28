//! PolarEffects desktop application.
//!
//! Rust owns the entire domain; the webview is a view layer. Everything the
//! frontend can reach goes through a Tauri command in [`commands`],
//! [`projects`], [`edit`] or [`autosave`], and every failure it can see is an
//! [`error::AppError`].

pub mod autosave;
pub mod commands;
pub mod edit;
pub mod error;
pub mod paths;
pub mod projects;
pub mod session;
pub mod settings;

/// Starts the application. Returns only when the last window closes.
pub fn run() -> anyhow::Result<()> {
    let paths = paths::AppPaths::resolve()?;
    tauri::Builder::default()
        .manage(commands::AppState::new(paths))
        .setup(|app| {
            autosave::start(app.handle().clone());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            projects::new_project,
            projects::open_project,
            projects::save_project,
            projects::save_project_as,
            projects::close_project,
            projects::project_summary,
            projects::recent_projects,
            projects::forget_recent_project,
            projects::clear_recent,
            edit::undo,
            edit::redo,
            edit::rename_project,
            autosave::recovered_projects,
            autosave::open_recovered,
            autosave::discard_recovered,
        ])
        .run(tauri::generate_context!())?;
    Ok(())
}
