#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Crash recovery (spec.md 4.5): a dirty project is snapshotted, an
//! unchanged one is not written again, fifty entries trigger before the
//! minute, a snapshot comes back as the project it was taken from, and a
//! clean save or a deliberate close forgets it.

mod common;

use common::TempRoot;
use pe_app::commands::AppState;
use pe_app::settings::AutosaveMode;
use pe_app::{autosave, edit, projects};

fn create(state: &AppState) -> u64 {
    projects::create(state, "Recoverable".to_owned(), None, true)
        .expect("create")
        .id
}

fn rename(state: &AppState, name: &str) {
    edit::rename(state, name.to_owned()).expect("rename");
}

#[test]
fn a_dirty_project_is_snapshotted_once_per_change() {
    let root = TempRoot::new("snapshot");
    let state = root.state();
    create(&state);
    assert!(
        autosave::snapshot(&state, false).unwrap(),
        "first look writes"
    );
    assert_eq!(autosave::list(&state).len(), 1);
    assert!(
        !autosave::snapshot(&state, true).unwrap(),
        "an unchanged project is not written again, even when forced"
    );
    rename(&state, "Changed");
    assert!(
        !autosave::snapshot(&state, false).unwrap(),
        "one edit seconds later: neither the minute nor fifty entries has passed"
    );
    assert!(autosave::snapshot(&state, true).unwrap());
    let listed = autosave::list(&state);
    assert_eq!(listed.len(), 1, "one snapshot per project, replaced");
    assert_eq!(listed[0].name, "Changed");
}

#[test]
fn fifty_entries_trigger_a_snapshot_before_the_minute() {
    let root = TempRoot::new("burst");
    let state = root.state();
    create(&state);
    assert!(autosave::snapshot(&state, true).unwrap());
    for i in 0..49 {
        rename(&state, &format!("Name {i}"));
    }
    assert!(!autosave::snapshot(&state, false).unwrap(), "49 is not 50");
    rename(&state, "Fiftieth");
    assert!(autosave::snapshot(&state, false).unwrap(), "50 is");
}

#[test]
fn a_clean_project_is_never_snapshotted() {
    let root = TempRoot::new("clean");
    let state = root.state();
    create(&state);
    projects::save_as(&state, root.file("clean.wpsproj")).unwrap();
    assert!(!autosave::snapshot(&state, true).unwrap());
    assert!(autosave::list(&state).is_empty());
}

#[test]
fn a_save_or_a_close_forgets_the_snapshot() {
    let root = TempRoot::new("forget");
    let state = root.state();
    create(&state);
    autosave::snapshot(&state, true).unwrap();
    projects::save_as(&state, root.file("saved.wpsproj")).unwrap();
    assert!(autosave::list(&state).is_empty(), "saved cleanly");

    rename(&state, "Edited");
    autosave::snapshot(&state, true).unwrap();
    assert_eq!(autosave::list(&state).len(), 1);
    projects::close(&state, true).unwrap();
    assert!(autosave::list(&state).is_empty(), "closed deliberately");
}

/// After a crash — a new state over the same directories — the snapshot is
/// offered and opens as the original project: dirty, at its original path.
#[test]
fn a_snapshot_survives_a_crash_and_opens_dirty_at_its_original_path() {
    let root = TempRoot::new("recover");
    let path = root.file("original.wpsproj");
    let id = {
        let state = root.state();
        let id = create(&state);
        projects::save_as(&state, path.clone()).unwrap();
        rename(&state, "Unsaved work");
        assert!(autosave::snapshot(&state, true).unwrap());
        id
        // Dropped without saving or closing: a crash.
    };

    let state = root.state();
    let offered = autosave::list(&state);
    assert_eq!(offered.len(), 1);
    assert_eq!(offered[0].id, id);
    assert_eq!(offered[0].name, "Unsaved work");
    assert_eq!(offered[0].original_path.as_deref(), Some(path.as_str()));

    let recovered = autosave::recover(&state, id, false).unwrap();
    assert_eq!(recovered.name, "Unsaved work");
    assert!(recovered.dirty);
    assert_eq!(recovered.path.as_deref(), Some(path.as_str()));

    // The next Save writes where the user meant, and the snapshot goes.
    projects::save(&state).unwrap();
    assert!(autosave::list(&state).is_empty());
    let reopened = pe_core::io::load(std::path::Path::new(&path)).unwrap();
    assert_eq!(reopened.name, "Unsaved work");
}

#[test]
fn a_never_saved_project_recovers_without_a_path() {
    let root = TempRoot::new("no-path");
    let id = {
        let state = root.state();
        let id = create(&state);
        autosave::snapshot(&state, true).unwrap();
        id
    };
    let state = root.state();
    let recovered = autosave::recover(&state, id, false).unwrap();
    assert!(recovered.path.is_none());
    assert!(matches!(
        projects::save(&state),
        Err(pe_app::error::AppError::ProjectNeverSaved)
    ));
}

#[test]
fn recovering_over_unsaved_work_is_guarded() {
    let root = TempRoot::new("recover-guard");
    let id = {
        let state = root.state();
        let id = create(&state);
        autosave::snapshot(&state, true).unwrap();
        id
    };
    let state = root.state();
    projects::create(&state, "Other".to_owned(), None, false).unwrap();
    assert!(autosave::recover(&state, id, false).is_err());
    assert!(autosave::recover(&state, id, true).is_ok());
}

#[test]
fn off_writes_nothing_and_save_writes_in_place() {
    let root = TempRoot::new("modes");
    let state = root.state();
    create(&state);
    state
        .with_session(|s| {
            s.settings.autosave = AutosaveMode::Off;
            Ok(())
        })
        .unwrap();
    assert!(!autosave::snapshot(&state, true).unwrap());
    assert!(autosave::list(&state).is_empty());

    let path = root.file("inplace.wpsproj");
    projects::save_as(&state, path.clone()).unwrap();
    state
        .with_session(|s| {
            s.settings.autosave = AutosaveMode::Save;
            Ok(())
        })
        .unwrap();
    rename(&state, "Saved in place");
    assert!(autosave::snapshot(&state, true).unwrap());
    assert!(
        autosave::list(&state).is_empty(),
        "no snapshot, the file itself"
    );
    assert!(!projects::summary(&state).unwrap().unwrap().dirty);
    let on_disk = pe_core::io::load(std::path::Path::new(&path)).unwrap();
    assert_eq!(on_disk.name, "Saved in place");
}

/// Review fix: the snapshot is written outside the session lock. A Save in
/// that gap forgets the snapshot, the write then puts it back, and a stale
/// snapshot would be offered as Recovered work. It must be removed.
#[test]
fn a_save_during_the_snapshot_write_leaves_no_stale_snapshot() {
    let root = TempRoot::new("race-save");
    let state = root.state();
    create(&state);
    projects::save_as(&state, root.file("race.wpsproj")).unwrap();
    rename(&state, "Edited");
    let kept = autosave::snapshot_with_hook(&state, true, || {
        projects::save(&state).expect("save in the gap");
    })
    .unwrap();
    assert!(!kept);
    assert!(
        autosave::list(&state).is_empty(),
        "stale snapshot survived a save"
    );
}

/// The same for a deliberate "Don't save" close in the gap: the snapshot
/// must not revive work the user chose to drop.
#[test]
fn a_close_during_the_snapshot_write_leaves_no_stale_snapshot() {
    let root = TempRoot::new("race-close");
    let state = root.state();
    create(&state);
    autosave::snapshot_with_hook(&state, true, || {
        projects::close(&state, true).expect("close in the gap");
    })
    .unwrap();
    assert!(
        autosave::list(&state).is_empty(),
        "a dropped project came back"
    );
}

/// Another project opened in the gap: the first one's snapshot is stale too.
#[test]
fn a_new_project_during_the_snapshot_write_leaves_no_stale_snapshot() {
    let root = TempRoot::new("race-new");
    let state = root.state();
    let first = create(&state);
    autosave::snapshot_with_hook(&state, true, || {
        projects::create(&state, "Next".to_owned(), None, true).expect("replace");
    })
    .unwrap();
    assert!(autosave::list(&state).iter().all(|r| r.id != first));
}

/// An edit (not a save) in the gap leaves the snapshot: it is older work of
/// the same unsaved project, and the next tick replaces it.
#[test]
fn an_edit_during_the_snapshot_write_keeps_the_snapshot() {
    let root = TempRoot::new("race-edit");
    let state = root.state();
    create(&state);
    assert!(autosave::snapshot_with_hook(&state, true, || rename(&state, "Later")).unwrap());
    assert_eq!(autosave::list(&state).len(), 1);
}
