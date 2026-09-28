#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Importing polar files over IPC (spec.md 6), driven through the function
//! the Tauri command calls.

mod common;

use common::TempRoot;
use pe_app::error::AppError;
use pe_app::{edit, polar_files, projects};
use pe_core::source::PALETTE;

const EXPEDITION: &[u8] = include_bytes!("../../pe-polar/tests/golden/expedition.txt");
const ADRENA: &[u8] = include_bytes!("../../pe-polar/tests/golden/adrena.pol");

fn write(root: &TempRoot, name: &str, bytes: &[u8]) -> String {
    let path = root.file(name);
    std::fs::write(&path, bytes).unwrap();
    path
}

/// A sample file checked in under `polar_examples/` (M9b, user request):
/// real-world files, not written for a test.
fn sample(relative: &str) -> String {
    format!(
        "{}/../../polar_examples/{relative}",
        env!("CARGO_MANIFEST_DIR")
    )
}

#[test]
fn a_batch_imports_what_it_can_as_one_undo_entry() {
    let root = TempRoot::new("polar-import");
    let app = root.state();
    projects::create(&app, "Import".to_owned(), None, false).unwrap();
    let paths = vec![
        write(&root, "boat.txt", EXPEDITION),
        write(&root, "broken.csv", b"TWA;6;8\n40;5;fast\n"),
        write(&root, "boat.pol", ADRENA),
        root.file("missing.pol"),
    ];

    let result = polar_files::import(&app, &paths).unwrap();
    assert_eq!(result.imported, ["boat.txt", "boat.pol"]);

    // The broken file names its line and column; the missing one cannot.
    assert_eq!(result.failures.len(), 2);
    let broken = &result.failures[0];
    assert_eq!(broken.file, "broken.csv");
    assert_eq!((broken.line, broken.column), (Some(2), Some(6)));
    assert_eq!(broken.reason, "not-a-number");
    assert!(broken.message.contains("broken.csv"), "{}", broken.message);
    let missing = &result.failures[1];
    assert_eq!((missing.file.as_str(), missing.line), ("missing.pol", None));
    assert_eq!(missing.reason, "unreadable");

    let project = &result.project;
    assert!(project.dirty);
    assert_eq!(project.sources.len(), 2);
    let (txt, pol) = (&project.sources[0], &project.sources[1]);
    assert_eq!(
        (txt.label.as_str(), txt.kind.as_str()),
        ("boat.txt", "polar_file")
    );
    assert_eq!(txt.colour, PALETTE[0]);
    assert_eq!(pol.colour, PALETTE[1]);
    assert_eq!(txt.count, 44);
    assert_eq!(pol.count, 71);
    let file = pol.polar_file.as_ref().unwrap();
    assert_eq!(file.format, "adrena");
    assert_eq!(file.tws, [6.0, 8.0, 10.0, 12.0, 16.0, 20.0]);
    assert_eq!(txt.polar_file.as_ref().unwrap().format, "expedition");
    assert_eq!(
        project.undo_label.as_deref(),
        Some(polar_files::IMPORT_MANY)
    );

    // One undo takes the whole batch out; redo puts it back.
    let undone = edit::undo_last(&app).unwrap();
    assert!(undone.sources.is_empty());
    assert!(!undone.can_undo);
    let redone = edit::redo_next(&app).unwrap();
    assert_eq!(redone.sources, project.sources);
}

#[test]
fn imported_polars_survive_a_save_and_reopen() {
    let root = TempRoot::new("polar-import-save");
    let app = root.state();
    projects::create(&app, "Saved".to_owned(), None, false).unwrap();
    let paths = [write(&root, "boat.txt", EXPEDITION)];
    let imported = polar_files::import(&app, &paths).unwrap().project;
    assert_eq!(
        imported.undo_label.as_deref(),
        Some(polar_files::IMPORT_ONE)
    );
    let file = root.file("saved.wpsproj");
    projects::save_as(&app, file.clone()).unwrap();
    projects::close(&app, false).unwrap();
    let reopened = projects::open(&app, file, false).unwrap();
    assert_eq!(reopened.sources, imported.sources);
}

#[test]
fn a_batch_where_nothing_imports_changes_nothing() {
    let root = TempRoot::new("polar-import-none");
    let app = root.state();
    let before = projects::create(&app, "None".to_owned(), None, false).unwrap();
    let paths = [write(&root, "notes.txt", b"hello\n")];
    let result = polar_files::import(&app, &paths).unwrap();
    assert!(result.imported.is_empty());
    assert_eq!(result.failures[0].reason, "unknown-format");
    assert_eq!(result.failures[0].line, Some(1));
    assert_eq!(result.project, before);
}

#[test]
fn importing_needs_an_open_project() {
    let root = TempRoot::new("polar-import-closed");
    let app = root.state();
    let paths = [write(&root, "boat.txt", EXPEDITION)];
    assert!(matches!(
        polar_files::import(&app, &paths),
        Err(AppError::NoProjectOpen)
    ));
}

/// A handful of the real-world sample polars in `polar_examples/` (M9b, user
/// request) import through the same IPC path as a person's own files: an
/// Adrena grid with a TWS axis to 70 kn, and two Expedition files whose
/// first row is a label row instead of data.
#[test]
fn sample_polars_import_through_the_same_ipc_path() {
    let root = TempRoot::new("polar-import-samples");
    let app = root.state();
    projects::create(&app, "Samples".to_owned(), None, false).unwrap();
    let paths = vec![
        sample("Polaires - Copy/VR_IMOCA.pol"),
        sample("polars/Swan 78.txt"),
        sample("polars/J35.txt"),
        sample("polars/J46 heel.txt"), // not a boat-speed polar: heel angles
    ];

    let result = polar_files::import(&app, &paths).unwrap();
    assert_eq!(result.imported, ["VR_IMOCA.pol", "Swan 78.txt", "J35.txt"]);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].file, "J46 heel.txt");
    assert_eq!(result.failures[0].reason, "negative");

    let project = &result.project;
    assert_eq!(project.sources.len(), 3);
    let imoca = &project.sources[0];
    assert_eq!(imoca.polar_file.as_ref().unwrap().format, "adrena");
    assert_eq!(imoca.polar_file.as_ref().unwrap().tws.last(), Some(&70.0));
    assert_eq!(
        project.sources[1].polar_file.as_ref().unwrap().format,
        "expedition"
    );
    assert_eq!(
        project.sources[2].polar_file.as_ref().unwrap().format,
        "expedition"
    );
    assert_eq!(
        project.undo_label.as_deref(),
        Some(polar_files::IMPORT_MANY)
    );
}

/// The interface translates failures by reason code; its list is every code
/// Rust can send.
#[test]
fn the_frontend_knows_every_failure_reason() {
    let listed: Vec<String> =
        serde_json::from_str(include_str!("../../../ui/src/panels/polar-reasons.json")).unwrap();
    let mut codes: Vec<&str> = pe_polar::Reason::CODES.to_vec();
    codes.push("unreadable");
    assert_eq!(listed, codes);
}
