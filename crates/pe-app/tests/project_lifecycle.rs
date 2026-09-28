#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! End-to-end project lifecycle (spec.md 3.3, 4.2–4.4), driving the same
//! functions the Tauri commands call.

mod common;

use common::TempRoot;
use pe_app::error::AppError;
use pe_app::projects::{self, BoatInput};
use pe_app::{edit, settings::Settings};

fn create(state: &pe_app::commands::AppState, name: &str) -> projects::ProjectSummary {
    projects::create(state, name.to_owned(), None, false).expect("create")
}

#[test]
fn a_project_survives_create_save_close_and_reopen() {
    let root = TempRoot::new("round-trip");
    let app = root.state();
    let path = root.file("demo.wpsproj");

    let created = projects::create(
        &app,
        "  Fastnet  ".to_owned(),
        Some(BoatInput {
            name: "Pen Duick".to_owned(),
            notes: "Ketch".to_owned(),
        }),
        false,
    )
    .expect("create");
    assert_eq!(created.name, "Fastnet");
    assert_eq!(created.boat_name, "Pen Duick");
    assert!(created.dirty, "a new project starts dirty");
    assert!(created.path.is_none());
    assert!(created.sources.is_empty());

    let saved = projects::save_as(&app, path.clone()).expect("save as");
    assert!(!saved.dirty);
    assert_eq!(saved.path.as_deref(), Some(path.as_str()));

    projects::close(&app, false).expect("a clean project closes without asking");
    assert!(projects::summary(&app).expect("summary").is_none());

    let reopened = projects::open(&app, path.clone(), false).expect("open");
    assert_eq!(reopened.name, "Fastnet");
    assert_eq!(reopened.boat_notes, "Ketch");
    assert_eq!(reopened.id, created.id);
    assert!(!reopened.dirty);
    assert!(!reopened.can_undo);
}

#[test]
fn a_blank_name_is_refused() {
    let root = TempRoot::new("blank");
    let app = root.state();
    assert!(matches!(
        projects::create(&app, "   ".to_owned(), None, false),
        Err(AppError::BadOption { .. })
    ));
    assert!(projects::summary(&app).unwrap().is_none());
}

#[test]
fn save_as_supplies_the_extension_and_save_reuses_the_path() {
    let root = TempRoot::new("extension");
    let app = root.state();
    create(&app, "NoExt");
    let saved = projects::save_as(&app, root.file("noext")).expect("save as");
    assert!(saved.path.as_deref().unwrap().ends_with(".wpsproj"));

    edit::rename(&app, "Renamed".to_owned()).expect("rename");
    let again = projects::save(&app).expect("save");
    assert_eq!(again.path, saved.path);
    assert!(!again.dirty);
}

/// Review fix: the extension is appended unless it is exactly `wpsproj`,
/// so a dotted name keeps its whole stem.
#[test]
fn save_as_appends_the_extension_unless_it_is_exactly_wpsproj() {
    let root = TempRoot::new("dotted");
    let app = root.state();
    create(&app, "Dotted");
    for (given, expected) in [
        ("Race.v2", "Race.v2.wpsproj"),
        ("Race.txt", "Race.txt.wpsproj"),
        ("Race.WPSPROJ", "Race.WPSPROJ.wpsproj"),
        ("Race.wpsproj", "Race.wpsproj"),
        ("Race", "Race.wpsproj"),
    ] {
        let saved = projects::save_as(&app, root.file(given)).expect("save as");
        assert_eq!(saved.path.as_deref(), Some(root.file(expected).as_str()));
        assert!(std::path::Path::new(&root.file(expected)).is_file());
    }
}

#[test]
fn saving_a_never_saved_project_asks_for_a_location() {
    let root = TempRoot::new("never-saved");
    let app = root.state();
    create(&app, "Fresh");
    assert!(matches!(
        projects::save(&app),
        Err(AppError::ProjectNeverSaved)
    ));
}

#[test]
fn operations_without_a_project_report_it() {
    let root = TempRoot::new("no-project");
    let app = root.state();
    assert!(matches!(projects::save(&app), Err(AppError::NoProjectOpen)));
    assert!(matches!(
        projects::save_as(&app, root.file("x.wpsproj")),
        Err(AppError::NoProjectOpen)
    ));
    assert!(matches!(
        edit::undo_last(&app),
        Err(AppError::NoProjectOpen)
    ));
    assert!(matches!(
        edit::rename(&app, "x".to_owned()),
        Err(AppError::NoProjectOpen)
    ));
}

/// Spec 3.3: the back end refuses to drop a dirty project unless the call
/// passes `discard_unsaved`, for every path that drops one.
#[test]
fn a_dirty_project_is_never_dropped_without_a_decision() {
    let root = TempRoot::new("guard");
    let app = root.state();
    let other = root.file("other.wpsproj");
    create(&app, "Other");
    projects::save_as(&app, other.clone()).unwrap();

    create(&app, "Dirty");
    let refusals = [
        projects::create(&app, "Second".to_owned(), None, false).map(|_| ()),
        projects::open(&app, other.clone(), false).map(|_| ()),
        projects::close(&app, false),
    ];
    for refusal in refusals {
        assert!(
            matches!(&refusal, Err(AppError::UnsavedChanges { name }) if name == "Dirty"),
            "{refusal:?}"
        );
    }
    assert_eq!(projects::summary(&app).unwrap().unwrap().name, "Dirty");

    // "Don't save" reaches the replacing call.
    let replaced = projects::open(&app, other, true).expect("discard and open");
    assert_eq!(replaced.name, "Other");
}

#[test]
fn an_edit_after_saving_makes_the_project_dirty_again() {
    let root = TempRoot::new("edit-dirty");
    let app = root.state();
    create(&app, "A");
    projects::save_as(&app, root.file("a.wpsproj")).unwrap();
    let edited = edit::rename(&app, "B".to_owned()).unwrap();
    assert!(edited.dirty);
    assert!(projects::close(&app, false).is_err());
    projects::close(&app, true).expect("close with a decision");
}

#[test]
fn undo_and_redo_reach_the_document() {
    let root = TempRoot::new("undo");
    let app = root.state();
    let first = create(&app, "Before");
    let renamed = edit::rename(&app, "After".to_owned()).unwrap();
    assert_eq!(renamed.name, "After");
    assert_eq!(renamed.undo_label.as_deref(), Some("Rename project"));
    assert!(renamed.revision > first.revision);

    let undone = edit::undo_last(&app).unwrap();
    assert_eq!(undone.name, "Before");
    assert!(undone.can_redo && !undone.can_undo);
    assert_eq!(undone.redo_label.as_deref(), Some("Rename project"));

    let redone = edit::redo_next(&app).unwrap();
    assert_eq!(redone.name, "After");

    // Renaming to the same name records nothing.
    let same = edit::rename(&app, "After".to_owned()).unwrap();
    assert_eq!(same.revision, redone.revision);
}

#[test]
fn recent_projects_persist_newest_first_and_mark_missing_files() {
    let root = TempRoot::new("recent");
    let (one, two) = (root.file("one.wpsproj"), root.file("two.wpsproj"));
    {
        let app = root.state();
        create(&app, "One");
        projects::save_as(&app, one.clone()).unwrap();
        create(&app, "Two");
        projects::save_as(&app, two.clone()).unwrap();
    }
    std::fs::remove_file(&one).unwrap();

    let restarted = root.state();
    let recent = projects::recent(&restarted).unwrap();
    assert_eq!(recent.len(), 2);
    assert_eq!(
        (recent[0].path.as_str(), recent[0].name.as_str()),
        (two.as_str(), "two")
    );
    assert!(recent[0].exists);
    assert!(!recent[1].exists, "a moved file is shown as not found");

    let after = projects::forget_recent(&restarted, one).unwrap();
    assert_eq!(after.len(), 1);
}

#[test]
fn the_recent_list_holds_ten_and_opening_moves_to_the_front() {
    let root = TempRoot::new("ten");
    let app = root.state();
    let paths: Vec<String> = (0..12)
        .map(|i| root.file(&format!("p{i}.wpsproj")))
        .collect();
    for path in &paths {
        projects::create(&app, "P".to_owned(), None, true).unwrap();
        projects::save_as(&app, path.clone()).unwrap();
    }
    let recent = projects::recent(&app).unwrap();
    assert_eq!(recent.len(), 10);
    assert_eq!(recent[0].path, paths[11]);

    projects::open(&app, paths[5].clone(), false).unwrap();
    assert_eq!(projects::recent(&app).unwrap()[0].path, paths[5]);
}

#[test]
fn a_cleared_recent_list_stays_cleared_after_a_restart() {
    let root = TempRoot::new("clear");
    let saved = root.file("one.wpsproj");
    {
        let app = root.state();
        create(&app, "One");
        projects::save_as(&app, saved.clone()).unwrap();
        assert!(projects::clear_recent_projects(&app).unwrap().is_empty());
    }
    assert!(projects::recent(&root.state()).unwrap().is_empty());
    assert!(std::path::Path::new(&saved).exists());
    let settings = Settings::load(&root.state().paths.settings_file());
    assert!(settings.recent_projects.is_empty());
}

#[test]
fn opening_a_file_that_is_not_a_project_fails_cleanly() {
    let root = TempRoot::new("junk");
    let app = root.state();
    let path = root.file("junk.wpsproj");
    std::fs::write(&path, b"not a project").unwrap();
    let err = projects::open(&app, path.clone(), false).unwrap_err();
    assert_eq!(err.kind(), "doing");
    assert!(err.to_string().contains(&path), "{err}");
    assert!(projects::summary(&app).unwrap().is_none());
}

/// Acceptance: a newer file is refused with a message naming both versions,
/// and with its own kind so the UI can say to update rather than "damaged".
#[test]
fn opening_a_newer_project_names_both_versions() {
    use std::io::Write;
    let root = TempRoot::new("newer");
    let app = root.state();
    let path = root.file("future.wpsproj");
    let mut zip = zip::ZipWriter::new(std::fs::File::create(&path).unwrap());
    zip.start_file("META-INF/version", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"99").unwrap();
    zip.start_file("project.json", zip::write::SimpleFileOptions::default())
        .unwrap();
    zip.write_all(b"{ a document from the future").unwrap();
    zip.finish().unwrap();
    let err = projects::open(&app, path, false).unwrap_err();
    assert_eq!(err.kind(), "schema-too-new");
    let message = err.to_string();
    assert!(message.contains("99"), "{message}");
    assert!(
        message.contains(&format!("up to version {}", pe_core::SCHEMA_VERSION)),
        "{message}"
    );
}
