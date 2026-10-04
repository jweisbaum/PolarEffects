//! Quitting and closing the window go through the unsaved-changes guard
//! (spec.md 3.3).
//!
//! The question is asked in the frontend, which owns the dialog, so every way
//! out — the Quit item, Cmd/Ctrl-Q, the window's close button, the platform's
//! own quit — is stopped here when the open project has unsaved changes, and
//! the frontend is told to ask. It answers with [`quit_app`], which checks
//! the answer the same way every other discard is checked and only then
//! exits.

use std::sync::atomic::Ordering;

use tauri::{Emitter, Manager};

use crate::commands::AppState;
use crate::error::Result;
use crate::session::Session;

/// The event that asks the frontend to run the guard.
pub const QUIT_REQUESTED: &str = "app://quit-requested";

/// Whether the application may close with this session: nothing is open, or
/// what is open has nothing unsaved.
pub fn nothing_to_lose(session: &Session) -> bool {
    session.open.as_ref().is_none_or(|open| !open.dirty)
}

/// Whether the application may close now: either there is nothing to lose,
/// or the user already answered the guard.
///
/// A poisoned session lock allows it: the session can no longer be saved
/// anyway, and trapping the user in a broken window helps nobody.
pub fn may_exit(state: &AppState) -> bool {
    state.exit_allowed.load(Ordering::SeqCst)
        || state
            .with_session(|session| Ok(nothing_to_lose(session)))
            .unwrap_or(true)
}

/// Asks the frontend to run the unsaved-changes guard.
pub fn ask<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let _ = app.emit(QUIT_REQUESTED, ());
}

/// The Quit menu item: exits at once if nothing would be lost, else asks.
pub fn request<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    if may_exit(&app.state::<AppState>()) {
        if !crate::catalogues::work_before_exit(app) {
            app.exit(0);
        }
    } else {
        ask(app);
    }
}

/// Records the user's answer to the guard. Refuses, as every other discard
/// does, when there are still unsaved changes and the answer was not
/// "Don't save" (spec.md 3.3), so no caller can quit past unsaved work by
/// forgetting to ask.
pub fn confirm(state: &AppState, discard_unsaved: bool) -> Result<()> {
    state.with_session(|session| {
        session.refuse_to_discard(discard_unsaved)?;
        // What the user chose to put down is not offered back (spec.md 4.5).
        if let Some(open) = &session.open {
            crate::autosave::forget(state, open.project.id.raw());
        }
        state.exit_allowed.store(true, Ordering::SeqCst);
        Ok(())
    })
}

/// Quits, once the user has answered the guard.
#[tauri::command]
pub fn quit_app(app: tauri::AppHandle, discard_unsaved: bool) -> Result<()> {
    confirm(&app.state::<AppState>(), discard_unsaved)?;
    if !crate::catalogues::work_before_exit(&app) {
        app.exit(0);
    }
    Ok(())
}
