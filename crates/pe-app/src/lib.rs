//! PolarEffects desktop application.
//!
//! Rust owns the entire domain; the webview is a view layer. Everything the
//! frontend can reach goes through [`commands`], and every failure it can see
//! is an [`error::AppError`].

pub mod commands;
pub mod error;

/// Starts the application. Returns only when the last window closes.
pub fn run() -> anyhow::Result<()> {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![commands::app_info])
        .run(tauri::generate_context!())?;
    Ok(())
}
