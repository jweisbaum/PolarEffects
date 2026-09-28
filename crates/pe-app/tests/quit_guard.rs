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

/// M2 review fix: an autosave tick already past its checks when the user
/// answers "Don't save" must not write the snapshot back after
/// `quit::confirm` forgot it, or the discarded work is offered as Recovered
/// on the next launch.
#[test]
fn dont_save_during_a_snapshot_write_leaves_no_snapshot() {
    let root = TempRoot::new("quit-race");
    let app = root.state();
    projects::create(&app, "Q".to_owned(), None, false).unwrap();
    let kept = autosave::snapshot_with_hook(&app, true, || {
        quit::confirm(&app, true).expect("don't save in the gap");
    })
    .unwrap();
    assert!(!kept);
    assert!(
        autosave::list(&app).is_empty(),
        "a discarded project came back"
    );
}

/// Once the guard is answered, later ticks write nothing at all.
#[test]
fn no_snapshot_is_taken_after_dont_save() {
    let root = TempRoot::new("quit-after");
    let app = root.state();
    projects::create(&app, "Q".to_owned(), None, false).unwrap();
    quit::confirm(&app, true).unwrap();
    assert!(!autosave::snapshot(&app, true).unwrap());
    assert!(autosave::list(&app).is_empty());
}

/// In Save mode the autosave writes the project file itself. "Don't save"
/// in the gap must leave the file as the user last saved it.
#[test]
fn dont_save_during_an_in_place_autosave_leaves_the_file_alone() {
    use pe_app::settings::AutosaveMode;
    let root = TempRoot::new("quit-inplace");
    let app = root.state();
    projects::create(&app, "Saved".to_owned(), None, false).unwrap();
    let path = root.file("q.wpsproj");
    projects::save_as(&app, path.clone()).unwrap();
    app.with_session(|s| {
        s.settings.autosave = AutosaveMode::Save;
        Ok(())
    })
    .unwrap();
    edit::rename(&app, "Discarded".to_owned()).unwrap();
    let kept = autosave::snapshot_with_hook(&app, true, || {
        quit::confirm(&app, true).expect("don't save in the gap");
    })
    .unwrap();
    assert!(!kept);
    let on_disk = pe_core::io::load(std::path::Path::new(&path)).unwrap();
    assert_eq!(on_disk.name, "Saved");
    assert!(autosave::list(&app).is_empty());
}
