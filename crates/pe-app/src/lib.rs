//! PolarEffects desktop application.
//!
//! Rust owns the entire domain; the webview is a view layer. Everything the
//! frontend can reach goes through a Tauri command in [`commands`],
//! [`projects`], [`edit`], [`env`], [`polar_files`], [`tracks`], [`trackers`], [`map_tracks`], [`polar_plot`], [`polar3d`], [`polar_edit`], [`blend`], [`compare`], [`orc`], [`settings`], [`basemap`],
//! [`quit`] or [`autosave`], and every failure it can see is an
//! [`error::AppError`].

pub mod autosave;
pub mod basemap;
pub mod blend;
pub mod commands;
pub mod compare;
pub mod derived;
pub mod edit;
pub mod env;
pub mod error;
pub mod grib;
pub mod map_tracks;
pub mod menu;
pub mod orc;
pub mod paths;
pub mod polar3d;
pub mod polar_edit;
pub mod polar_files;
pub mod polar_plot;
pub mod projects;
pub mod quit;
pub mod session;
pub mod settings;
pub mod trackers;
pub mod tracks;

use tauri::Manager;

/// Starts the application. Returns only when the application exits.
pub fn run() -> anyhow::Result<()> {
    let paths = paths::AppPaths::resolve()?;
    // Fail loudly at startup rather than showing a blank map later.
    basemap::inspect(basemap::EMBEDDED)?;
    let builder = tauri::Builder::default().plugin(tauri_plugin_dialog::init());
    // The WebDriver endpoint (D25): an inbound socket on loopback that drives
    // the whole interface — invariant 4 in the other direction — so a shipped
    // build must not carry it, and the feature being off by default is what
    // makes sure of that (`tests/webdriver_optional.rs`).
    #[cfg(feature = "webdriver")]
    let builder = {
        eprintln!(
            "the WebDriver automation endpoint is compiled in; this build is for \
             testing and must not be shipped"
        );
        builder.plugin(tauri_plugin_webdriver_automation::init())
    };
    let app = builder
        .manage(commands::AppState::new(paths))
        .setup(|app| {
            let language = app
                .state::<commands::AppState>()
                .with_session(|session| Ok(session.settings.language.clone()))
                .unwrap_or_else(|_| settings::LANGUAGES[0].to_owned());
            app.set_menu(menu::build(app.handle(), &language)?)?;
            autosave::start(app.handle().clone());
            env::start(app.handle().clone());
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
            polar_plot::polar_plot,
            polar_plot::polar_plot_dots,
            tracks::inspect_track_files,
            tracks::inspect_csv_track,
            tracks::import_track_files,
            tracks::set_track_filters,
            tracks::set_track_derivation,
            tracks::sample_details,
            trackers::tracker_event,
            trackers::cancel_tracker_event,
            trackers::import_tracker_boats,
            env::env_estimate,
            env::start_env_fetch,
            env::cancel_env_fetch,
            env::env_jobs,
            env::set_use_corrected,
            env::set_stokes_drift,
            grib::grib_preview,
            grib::start_grib_export,
            grib::cancel_grib_export,
            grib::grib_export_status,
            map_tracks::map_tracks,
            polar3d::polar_scene,
            polar3d::set_excluded,
            polar_edit::polar_edit_surface,
            polar_edit::edit_polar,
            polar_edit::set_segment_statistic,
            blend::set_blend_visible,
            blend::set_blend_colour,
            blend::set_blend_settings,
            blend::export_preview,
            blend::export_polar,
            compare::compare_polars,
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
            settings::set_weather_memory,
            settings::set_network,
            settings::set_projection,
            settings::set_plot_band,
            settings::legacy_cache_notice,
            settings::remove_old_chunk_cache,
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
