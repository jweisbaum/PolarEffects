#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! One cell of the blend with what stands behind it (spec.md 10.1, 12.3):
//! what the blend surface's tooltip shows. Read-only and derived; nothing is
//! written into the project.

mod common;

use common::TempRoot;
use pe_app::blend::{self, BlendCellOrigin};
use pe_app::commands::AppState;
use pe_app::polar_edit::{self, EditOp, PolarCell};
use pe_app::{edit, projects};

fn app(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Blend cell".to_owned(), None, false).unwrap();
    common::add_blend_sources(&app);
    app
}

/// Indices of a TWA and TWS on the default output grid.
fn at(app: &AppState, twa: f64, tws: f64) -> (u32, u32) {
    app.with_session(|session| {
        let grid = &session.require_open()?.project.grid;
        Ok((
            grid.twa.iter().position(|v| *v == twa).unwrap() as u32,
            grid.tws.iter().position(|v| *v == tws).unwrap() as u32,
        ))
    })
    .unwrap()
}

fn project_bytes(app: &AppState) -> Vec<u8> {
    app.with_session(|s| Ok(pe_core::io::to_bytes(&s.require_open()?.project).unwrap()))
        .unwrap()
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The file at weight 0.5 (8.0 kn) and the track at weight 1 with a third
/// of full confidence (6.81 kn): weights 0.5 and 1/3, so shares 0.6 and
/// 0.4, and the blend is 0.6 × 8.0 + 0.4 × 6.81 = 7.524 kn.
#[test]
fn a_direct_cell_names_each_source_its_value_and_its_share() {
    let root = TempRoot::new("blend-cell");
    let app = app(&root);
    edit::source_weight_set(&app, 1, 0.5, None).unwrap();
    let before = project_bytes(&app);

    let (i, j) = at(&app, 90.0, 12.0);
    let cell = blend::blend_cell_of(&app, i, j).unwrap();
    assert_eq!((cell.twa, cell.tws), (90.0, 12.0));
    assert_eq!(cell.bsp, Some(7.524));
    assert_eq!(cell.origin, BlendCellOrigin::Direct);
    assert!(!cell.corrected);
    assert_eq!(
        cell.contributors
            .iter()
            .map(|c| c.source_id)
            .collect::<Vec<_>>(),
        [1, 2]
    );
    let (file, track) = (&cell.contributors[0], &cell.contributors[1]);
    assert_eq!((file.bsp, track.bsp), (8.0, 6.81));
    assert!(close(file.weight, 0.5) && close(track.weight, 1.0 / 3.0));
    assert!(close(file.share, 0.6) && close(track.share, 0.4));

    assert_eq!(project_bytes(&app), before, "reading a cell writes nothing");
}

/// A hidden source and a source at weight 0 stand behind nothing.
#[test]
fn hidden_and_weightless_sources_do_not_contribute() {
    let root = TempRoot::new("blend-cell-hidden");
    let app = app(&root);
    let (i, j) = at(&app, 90.0, 12.0);

    edit::source_weight_set(&app, 2, 0.0, None).unwrap();
    let cell = blend::blend_cell_of(&app, i, j).unwrap();
    assert_eq!(cell.bsp, Some(8.0));
    assert_eq!(cell.contributors.len(), 1);
    assert_eq!(cell.contributors[0].source_id, 1);
    assert!(close(cell.contributors[0].share, 1.0));

    edit::source_visible_set(&app, 1, false).unwrap();
    let cell = blend::blend_cell_of(&app, i, j).unwrap();
    assert_eq!(cell.bsp, None);
    assert_eq!(cell.origin, BlendCellOrigin::Empty);
    assert!(cell.contributors.is_empty());
}

/// Outside every source's coverage there is no value and no contributor;
/// the 0° row is 0 kn by definition, filled, with no one behind it.
#[test]
fn an_empty_cell_and_the_zero_row_have_no_contributors() {
    let root = TempRoot::new("blend-cell-empty");
    let app = app(&root);
    let (i, j) = at(&app, 30.0, 6.0);
    let cell = blend::blend_cell_of(&app, i, j).unwrap();
    assert_eq!(cell.bsp, None);
    assert_eq!(cell.origin, BlendCellOrigin::Empty);
    assert!(cell.contributors.is_empty());

    let (i, j) = at(&app, 0.0, 10.0);
    let cell = blend::blend_cell_of(&app, i, j).unwrap();
    assert_eq!(cell.bsp, Some(0.0));
    assert_eq!(cell.origin, BlendCellOrigin::Filled);
    assert!(cell.contributors.is_empty());
}

/// A manual correction of the blend (spec.md 12.3) holds the cell: the
/// value is the correction, flagged, and the sources behind the derived
/// value are still named.
#[test]
fn a_corrected_cell_says_so_and_still_names_its_sources() {
    let root = TempRoot::new("blend-cell-corrected");
    let app = app(&root);
    let (i, j) = at(&app, 90.0, 12.0);
    // Source 0 is the blend: the edit is stored as a correction overlay.
    polar_edit::polar_edit(
        &app,
        0,
        &EditOp::Type { bsp: Some(9.25) },
        &[PolarCell {
            twa_index: i,
            tws_index: j,
        }],
        None,
    )
    .unwrap();
    let cell = blend::blend_cell_of(&app, i, j).unwrap();
    assert_eq!(cell.bsp, Some(9.25));
    assert!(cell.corrected);
    assert_eq!(cell.origin, BlendCellOrigin::Direct);
    assert_eq!(cell.contributors.len(), 2);
}

#[test]
fn a_cell_off_the_grid_is_refused() {
    let root = TempRoot::new("blend-cell-range");
    let app = app(&root);
    let error = blend::blend_cell_of(&app, 99, 0).unwrap_err();
    assert_eq!(error.kind(), "bad-option");
    assert!(blend::blend_cell_of(&app, 0, 99).is_err());
}
