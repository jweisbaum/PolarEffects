#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Quitting goes through the unsaved-changes guard (spec.md 3.3): the
//! application may exit at once only when nothing would be lost, and the
//! user's answer is checked the way every other discard is.

mod common;

use common::TempRoot;
use pe_app::error::AppError;
use pe_app::{autosave, edit, projects, quit};

#[test]
fn nothing_open_or_nothing_unsaved_quits_at_once() {
    let root = TempRoot::new("quit-clean");
    let app = root.state();
    assert!(quit::may_exit(&app), "start screen");

    projects::create(&app, "Q".to_owned(), None, false).unwrap();
    projects::save_as(&app, root.file("q.wpsproj")).unwrap();
    assert!(quit::may_exit(&app), "saved project");
}

#[test]
fn unsaved_work_stops_the_exit_until_the_user_answers() {
    let root = TempRoot::new("quit-dirty");
    let app = root.state();
    projects::create(&app, "Q".to_owned(), None, false).unwrap();
    assert!(!quit::may_exit(&app), "a new project starts dirty");

    // Cancel is simply not answering: the exit stays stopped.
    assert!(!quit::may_exit(&app));

    // An answer that is not "Don't save" is refused while work is unsaved.
    assert!(matches!(
        quit::confirm(&app, false),
        Err(AppError::UnsavedChanges { .. })
    ));
    assert!(!quit::may_exit(&app));

    // "Don't save" is accepted.
    quit::confirm(&app, true).unwrap();
    assert!(quit::may_exit(&app));
}

#[test]
fn saving_first_then_quitting_needs_no_discard() {
    let root = TempRoot::new("quit-save");
    let app = root.state();
    projects::create(&app, "Q".to_owned(), None, false).unwrap();
    edit::rename(&app, "Renamed".to_owned()).unwrap();
    projects::save_as(&app, root.file("q.wpsproj")).unwrap();
    quit::confirm(&app, false).unwrap();
    assert!(quit::may_exit(&app));
}

#[test]
fn quitting_without_saving_drops_the_recovery_snapshot() {
    let root = TempRoot::new("quit-snapshot");
    let app = root.state();
    projects::create(&app, "Q".to_owned(), None, false).unwrap();
    assert!(autosave::snapshot(&app, true).unwrap());
    assert_eq!(autosave::list(&app).len(), 1);
    quit::confirm(&app, true).unwrap();
    assert!(autosave::list(&app).is_empty());
}
