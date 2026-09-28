//! PolarEffects desktop application.
//!
//! Rust owns the entire domain; the webview is a view layer. Everything the
//! frontend can reach goes through a Tauri command in [`commands`],
//! [`projects`], [`edit`], [`polar_files`], [`orc`], [`settings`], [`basemap`], [`quit`] or
//! [`autosave`], and every failure it can see is an [`error::AppError`].

pub mod autosave;
pub mod basemap;
pub mod commands;
pub mod edit;
pub mod error;
pub mod menu;
pub mod orc;
pub mod paths;
pub mod polar_files;
pub mod projects;
pub mod quit;
pub mod session;
pub mod settings;

use tauri::Manager;

/// Starts the application. Returns only when the application exits.
pub fn run() -> anyhow::Result<()> {
    let paths = paths::AppPaths::resolve()?;
    // Fail loudly at startup rather than showing a blank map later.
    basemap::inspect(basemap::EMBEDDED)?;
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .manage(commands::AppState::new(paths))
        .setup(|app| {
            let language = app
                .state::<commands::AppState>()
                .with_session(|session| Ok(session.settings.language.clone()))
                .unwrap_or_else(|_| settings::LANGUAGES[0].to_owned());
            app.set_menu(menu::build(app.handle(), &language)?)?;
            autosave::start(app.handle().clone());
            Ok(())
        })
        .on_menu_event(|app, event| match event.id().as_ref() {
            menu::QUIT_ID => quit::request(app),
            menu::HELP_ID => {
                use tauri::Emitter;
                let _ = app.emit("help://open", ());
            }
            _ => {}
        })
        // The window's close button, and Close Window, ask first too.
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event
                && !quit::may_exit(&window.state::<commands::AppState>())
            {
                api.prevent_close();
                quit::ask(window.app_handle());
            }
        })
        .invoke_handler(tauri::generate_handler![
            commands::app_info,
            basemap::basemap,
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
            edit::set_source_colour,
            edit::set_source_visible,
            edit::set_source_weight,
            edit::set_source_label,
            edit::move_source,
            edit::remove_source,
            polar_files::import_polar_files,
            orc::orc_catalogue_info,
            orc::orc_search,
            orc::orc_add,
            autosave::recovered_projects,
            autosave::open_recovered,
            autosave::discard_recovered,
            settings::app_settings,
            settings::set_language,
            settings::set_theme,
            settings::set_units,
            settings::set_autosave_mode,
            settings::set_chunk_cache,
            settings::set_network,
            settings::set_projection,
            settings::chunk_cache_status,
            settings::clear_chunk_cache,
            quit::quit_app,
        ])
        .build(tauri::generate_context!())?;
    app.run(|app, event| {
        // The platform's own quit (the Dock, logging out) arrives here with
        // no exit code; `app.exit` from `quit_app` arrives with one.
        if let tauri::RunEvent::ExitRequested {
            code: None, api, ..
        } = event
            && !quit::may_exit(&app.state::<commands::AppState>())
        {
            api.prevent_exit();
            quit::ask(app);
        }
    });
    Ok(())
}
