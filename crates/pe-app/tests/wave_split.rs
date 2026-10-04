#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Split Wave Angle's blends over IPC (spec.md 10.5): the blend of each
//! copy as a packet of surfaces, and one cell of a copy's blend with what
//! stands behind it. Derived and read-only: nothing is written into the
//! project.

mod common;

use common::TempRoot;
use pe_app::blend::{self, BlendCellOrigin};
use pe_app::commands::AppState;
use pe_app::polar3d::{self, SPLIT_MAGIC, SPLIT_VERSION};
use pe_app::wave_split::{WaveSense, WaveSplit};
use pe_app::{edit, projects};

const FOUR: WaveSplit = WaveSplit {
    count: 4,
    sense: WaveSense::From,
};

/// The blend fixture's file (8.0 kn at 90° in 12 kn) and track (ten
/// samples there, 6.0–6.9 kn), the track's first five samples with waves
/// on the bow and the rest on the starboard beam.
fn app(root: &TempRoot) -> AppState {
    let app = root.state();
    projects::create(&app, "Wave split".to_owned(), None, false).unwrap();
    common::add_blend_sources(&app);
    app.with_session(|session| {
        let open = session.require_open()?;
        let track = open
            .project
            .sources
            .iter_mut()
            .find(|source| source.id.raw() == 2)
            .expect("the track");
        if let pe_core::SourceKind::Track { track } = &mut track.kind {
            for (k, sample) in track.samples.iter_mut().enumerate() {
                sample.wave_from = Some(if k < 5 { 0.0 } else { 90.0 });
                sample.hs_m = Some(1.0);
            }
        }
        open.derived.invalidate_all();
        Ok(())
    })
    .unwrap();
    app
}

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

fn word(bytes: &[u8], k: usize) -> u32 {
    u32::from_le_bytes(bytes[k * 4..k * 4 + 4].try_into().unwrap())
}

fn float(bytes: &[u8], k: usize) -> f32 {
    f32::from_le_bytes(bytes[k * 4..k * 4 + 4].try_into().unwrap())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() < 1e-9
}

/// The file at full confidence and weight 1, the track at weight 1 with
/// five of thirty samples for full confidence (1/6). On the bow the track
/// says the 90th percentile of 6.0–6.4, 6.36: the blend is
/// (8.0 + 6.36/6) / (7/6) = 54.36/7 kn. On the beam, 6.5–6.9 give 6.86 and
/// 54.86/7 kn. Astern and to port only the file speaks: 8.0 kn.
#[test]
fn each_copy_has_its_own_blend_and_the_packet_carries_them_all() {
    let root = TempRoot::new("wave-split-packet");
    let app = app(&root);
    let before = project_bytes(&app);

    let bytes = polar3d::split_bytes(&app, FOUR).unwrap();
    assert_eq!(word(&bytes, 0), SPLIT_MAGIC);
    assert_eq!(word(&bytes, 1), SPLIT_VERSION);
    assert_eq!(word(&bytes, 2), 4);
    assert_eq!(word(&bytes, 3), 4, "the file gives every copy a blend");
    let mut offset = 4;
    let mut seen = Vec::new();
    for _ in 0..4 {
        let (cell, ni, nj) = (
            word(&bytes, offset),
            word(&bytes, offset + 1) as usize,
            word(&bytes, offset + 2) as usize,
        );
        offset += 3;
        let twa: Vec<f32> = (0..ni).map(|k| float(&bytes, offset + k)).collect();
        let tws: Vec<f32> = (0..nj).map(|k| float(&bytes, offset + ni + k)).collect();
        let i = twa.iter().position(|v| *v == 90.0).unwrap();
        let j = tws.iter().position(|v| *v == 12.0).unwrap();
        seen.push((
            cell,
            f64::from(float(&bytes, offset + ni + nj + i * nj + j)),
        ));
        offset += ni + nj + ni * nj;
    }
    assert_eq!(offset * 4, bytes.len(), "nothing after the last surface");
    assert_eq!(
        seen.iter().map(|(cell, _)| *cell).collect::<Vec<_>>(),
        [0, 1, 2, 3]
    );
    let bsp = |cell: usize| seen[cell].1;
    assert!((bsp(0) - 54.36 / 7.0).abs() < 1e-5, "bow {}", bsp(0));
    assert!((bsp(1) - 54.86 / 7.0).abs() < 1e-5, "beam {}", bsp(1));
    assert!((bsp(2) - 8.0).abs() < 1e-5 && (bsp(3) - 8.0).abs() < 1e-5);

    // To: the same two directions, half a turn round.
    let to = polar3d::split_bytes(
        &app,
        WaveSplit {
            count: 4,
            sense: WaveSense::To,
        },
    )
    .unwrap();
    assert_eq!(word(&to, 4), 0, "the first surface is still copy 0");
    // A count the slider does not offer is refused.
    assert!(polar3d::split_bytes(&app, WaveSplit { count: 5, ..FOUR }).is_err());
    assert_eq!(
        project_bytes(&app),
        before,
        "reading the split writes nothing"
    );
}

#[test]
fn a_cell_of_a_copy_names_the_sources_behind_it_there() {
    let root = TempRoot::new("wave-split-cell");
    let app = app(&root);
    let (i, j) = at(&app, 90.0, 12.0);
    // Through the command's own function.
    let bow = blend::blend_cell_split_of(&app, i, j, FOUR, 0).unwrap();
    assert_eq!((bow.twa, bow.tws), (90.0, 12.0));
    // The cell's speed is rounded to six decimals.
    assert!(
        (bow.bsp.unwrap() - 54.36 / 7.0).abs() < 1e-6,
        "{:?}",
        bow.bsp
    );
    assert_eq!(bow.origin, BlendCellOrigin::Direct);
    let (file, track) = (&bow.contributors[0], &bow.contributors[1]);
    assert_eq!((file.source_id, track.source_id), (1, 2));
    assert_eq!((file.bsp, track.bsp), (8.0, 6.36));
    assert!(close(file.weight, 1.0) && close(track.weight, 1.0 / 6.0));
    let beam = blend::blend_cell_split_of(&app, i, j, FOUR, 1).unwrap();
    assert_eq!(beam.contributors[1].bsp, 6.86);
    let astern = blend::blend_cell_split_of(&app, i, j, FOUR, 2).unwrap();
    assert_eq!(astern.bsp, Some(8.0));
    assert_eq!(astern.contributors.len(), 1, "no sample from astern");
    // The whole blend is what it was: all ten samples, a third of confidence.
    let whole = blend::blend_cell_of(&app, i, j).unwrap();
    assert!(close(whole.contributors[1].weight, 1.0 / 3.0));
    // Hiding the file leaves the copies from astern with nothing to say.
    edit::source_visible_set(&app, 1, false).unwrap();
    let astern = blend::blend_cell_split_of(&app, i, j, FOUR, 2).unwrap();
    assert_eq!(astern.bsp, None);
    assert!(astern.contributors.is_empty());
    assert_eq!(word(&polar3d::split_bytes(&app, FOUR).unwrap(), 3), 2);
    // A copy the split does not have is refused.
    assert!(blend::blend_cell_split_of(&app, i, j, FOUR, 4).is_err());
}
