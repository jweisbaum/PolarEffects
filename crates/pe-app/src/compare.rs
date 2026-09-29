//! The Compare stage over IPC (spec.md 11).
//!
//! The frontend names two operands — a polar source's polar, a track's
//! polar segment, or the blend — and a threshold; Rust reads both onto the
//! project output grid **as the other views see them** (overlays applied,
//! nothing extrapolated) and compares them with `pe_polar::compare`. The UI
//! never computes a difference: it draws what arrives.
//!
//! How an operand is read:
//!
//! - a polar source (ORC or file): its grid with its edits written in, read
//!   bilinearly onto the output grid with every cell read from an excluded
//!   node empty — exactly what the blend reads (spec.md 12.3);
//! - a track: its segment on the output grid, overrides written in;
//! - the blend: the blend grid of every visible source (spec.md 12.3).
//!
//! A source may be compared while hidden: hiding takes it out of the blend
//! and the plots, not out of reach.
//!
//! Everything is derived from the session's cache (never persisted,
//! invariant 2), so an edit costs one comparison of two grids.
//!
//! # Wire layout, version 2
//!
//! One binary buffer, not JSON: a 512 × 512 output grid is 262,144 cells,
//! over 1.3 million values across the five per-cell arrays. The frontend's mirror is `ui/src/compare/comparePacket.ts`,
//! and both are held to the same bytes by
//! `ui/src/compare/fixtures/compare-v2.bin`. Every value is little-endian
//! and 4 bytes wide.
//!
//! ```text
//! header, 20 × u32 (80 bytes)
//!   0  magic       0x4d434550 (the bytes "PECM")
//!   1  version     2
//!   2  ni          TWA values
//!   3  nj          TWS values
//!   4  R           regions
//!   5  overlap     cells both cover
//!   6  a_only      cells only A covers
//!   7  b_only      cells only B covers
//!   8  u32 the cell of the largest |Δ| kn, i × nj + j (0xFFFFFFFF: none)
//!   9  u32 the cell of the largest |Δ| %
//!   10 f32 threshold, kn
//!   11–14 f32 mean |Δ|, max |Δ|, min Δ, max Δ, kn (NaN: no compared cell)
//!   15–18 f32 the same in percent of B
//!   19 pct_excluded compared cells with no percentage (B under 0.1 kn)
//! f32 [ni] TWA axis, f32 [nj] TWS axis
//! f32 [ni × nj] A, then B, then Δ kn, then Δ %; TWA-major (i × nj + j),
//!               NaN for no value
//! u32 [ni × nj] class: 0 neither, 1 A only, 2 B only, 3 both, 4 the 0° row
//! regions, R × 4 u32: TWS index, first TWA index, last TWA index,
//!               faster (1 A, 2 B)
//! ```

use pe_core::source::SourceKind;
use pe_core::{Project, SourceId};
use pe_polar::blend::on_grid;
use pe_polar::{CellClass, Comparison, Faster, Polar};
use serde::Deserialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::derived::Derivations;
use crate::error::{AppError, Result};

/// "PECM" read as a little-endian u32.
pub const COMPARE_MAGIC: u32 = u32::from_le_bytes(*b"PECM");
/// The wire layout's version.
pub const COMPARE_VERSION: u32 = 2;
/// Header length in bytes.
pub const HEADER_BYTES: usize = 80;
/// No cell.
pub const NO_CELL: u32 = u32::MAX;

/// One side of a comparison (spec.md 11).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize, TS)]
#[serde(tag = "kind", rename_all = "snake_case")]
#[ts(export_to = "CompareOperand.ts")]
pub enum CompareOperand {
    /// The current blend.
    Blend,
    /// An ORC or polar-file source's polar, as edited.
    Polar {
        /// The source.
        source_id: u64,
    },
    /// A track's polar segment, as edited.
    Segment {
        /// The source.
        source_id: u64,
    },
}

fn bad(operand: CompareOperand, why: &str) -> AppError {
    AppError::BadOption {
        field: "Compare operand",
        value: format!("{operand:?}: {why}"),
    }
}

/// `operand` on the project output grid, as the other views read it.
pub fn operand_grid(
    project: &Project,
    derivations: &mut Derivations,
    operand: CompareOperand,
) -> Result<Polar> {
    let grid = &project.grid;
    let (id, want_track) = match operand {
        CompareOperand::Blend => return Ok(derivations.blend(project).polar.clone()),
        CompareOperand::Polar { source_id } => (source_id, false),
        CompareOperand::Segment { source_id } => (source_id, true),
    };
    let source = project
        .source(SourceId(id))
        .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))?;
    let is_track = matches!(source.kind, SourceKind::Track { .. });
    if is_track != want_track {
        return Err(bad(
            operand,
            if is_track {
                "a track has a segment, not a polar"
            } else {
                "only a track has a segment"
            },
        ));
    }
    let data = derivations.get(project, source);
    Ok(if is_track {
        data.blend.clone()
    } else {
        on_grid(&data.edited, &source.overlay, &grid.twa, &grid.tws)
    })
}

/// Compares `a` with `b` in `project` (spec.md 11).
pub fn compare_in(
    project: &Project,
    derivations: &mut Derivations,
    a: CompareOperand,
    b: CompareOperand,
    threshold_kn: f64,
) -> Result<Comparison> {
    let a = operand_grid(project, derivations, a)?;
    let b = operand_grid(project, derivations, b)?;
    pe_polar::compare(&a, &b, threshold_kn).map_err(|error| AppError::BadOption {
        field: "Compare threshold",
        value: error.to_string(),
    })
}

fn class_code(class: CellClass) -> u32 {
    match class {
        CellClass::Neither => 0,
        CellClass::AOnly => 1,
        CellClass::BOnly => 2,
        CellClass::Both => 3,
        CellClass::ZeroRow => 4,
    }
}

fn narrow(value: Option<f64>) -> f32 {
    value.map_or(f32::NAN, |v| v as f32)
}

/// Packs a comparison into the wire layout in the module documentation.
pub fn pack(c: &Comparison) -> Vec<u8> {
    let (ni, nj) = (c.twa.len(), c.tws.len());
    let cells = ni * nj;
    let mut out =
        Vec::with_capacity(HEADER_BYTES + (ni + nj + cells * 5 + c.regions.len() * 4) * 4);
    let u = |out: &mut Vec<u8>, value: u32| out.extend_from_slice(&value.to_le_bytes());
    let f = |out: &mut Vec<u8>, value: f32| out.extend_from_slice(&value.to_le_bytes());
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    let at =
        |cell: Option<(f64, usize, usize)>| cell.map_or(NO_CELL, |(_, i, j)| count(i * nj + j));

    u(&mut out, COMPARE_MAGIC);
    u(&mut out, COMPARE_VERSION);
    u(&mut out, count(ni));
    u(&mut out, count(nj));
    u(&mut out, count(c.regions.len()));
    u(&mut out, count(c.overlap));
    u(&mut out, count(c.a_only));
    u(&mut out, count(c.b_only));
    u(&mut out, at(c.kn.max_abs));
    u(&mut out, at(c.pct.max_abs));
    f(&mut out, c.threshold_kn as f32);
    for stats in [&c.kn, &c.pct] {
        f(&mut out, narrow(stats.mean_abs));
        f(&mut out, narrow(stats.max_abs.map(|m| m.0)));
        f(&mut out, narrow(stats.range.map(|r| r.0)));
        f(&mut out, narrow(stats.range.map(|r| r.1)));
    }
    u(&mut out, count(c.pct_excluded));

    for value in c.twa.iter().chain(&c.tws) {
        f(&mut out, *value as f32);
    }
    for grid in [&c.a, &c.b, &c.delta_kn, &c.delta_pct] {
        for row in grid {
            for value in row {
                f(&mut out, narrow(*value));
            }
        }
    }
    for row in &c.class {
        for class in row {
            u(&mut out, class_code(*class));
        }
    }
    for region in &c.regions {
        u(&mut out, count(region.tws_index));
        u(&mut out, count(region.first_twa_index));
        u(&mut out, count(region.last_twa_index));
        u(
            &mut out,
            match region.faster {
                Faster::A => 1,
                Faster::B => 2,
            },
        );
    }
    out
}

/// The comparison of `a` and `b` in the open project, packed (see the
/// module documentation). `threshold_kn` splits "A faster" and "B faster"
/// regions from "about the same" (spec.md 11; 0.05 kn by default).
#[tauri::command]
pub fn compare_polars(
    state: tauri::State<'_, AppState>,
    a: CompareOperand,
    b: CompareOperand,
    threshold_kn: f64,
) -> Result<tauri::ipc::Response> {
    compare_bytes(&state, a, b, threshold_kn).map(tauri::ipc::Response::new)
}

/// [`compare_polars`] without a Tauri handle.
pub fn compare_bytes(
    state: &AppState,
    a: CompareOperand,
    b: CompareOperand,
    threshold_kn: f64,
) -> Result<Vec<u8>> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let comparison = compare_in(&open.project, &mut open.derived, a, b, threshold_kn)?;
        Ok(pack(&comparison))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use pe_polar::Region;
    use pe_polar::compare::DeltaStats;

    use super::*;

    /// A 2 × 2 comparison with one of each class but "neither", and one
    /// region, so the whole layout is pinned.
    fn fixture() -> Comparison {
        Comparison {
            twa: vec![0.0, 90.0],
            tws: vec![6.0, 12.0],
            a: vec![vec![Some(0.0), Some(0.0)], vec![Some(7.5), Some(8.0)]],
            b: vec![vec![Some(0.0), None], vec![Some(7.0), None]],
            delta_kn: vec![vec![None, None], vec![Some(0.5), None]],
            delta_pct: vec![vec![None, None], vec![Some(7.142857), None]],
            class: vec![
                vec![CellClass::ZeroRow, CellClass::ZeroRow],
                vec![CellClass::Both, CellClass::AOnly],
            ],
            overlap: 1,
            a_only: 1,
            b_only: 0,
            kn: DeltaStats {
                mean_abs: Some(0.5),
                max_abs: Some((0.5, 1, 0)),
                range: Some((0.5, 0.5)),
            },
            pct: DeltaStats {
                mean_abs: Some(7.142857),
                max_abs: Some((7.142857, 1, 0)),
                range: Some((7.142857, 7.142857)),
            },
            pct_excluded: 3,
            threshold_kn: 0.05,
            regions: vec![Region {
                tws_index: 0,
                first_twa_index: 1,
                last_twa_index: 1,
                faster: Faster::A,
            }],
        }
    }

    fn word(bytes: &[u8], index: usize) -> u32 {
        u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    }

    fn float(bytes: &[u8], index: usize) -> f32 {
        f32::from_bits(word(bytes, index))
    }

    /// Offsets computed by hand from the layout, not from `pack`.
    #[test]
    fn the_layout_is_the_documented_one() {
        let bytes = pack(&fixture());
        // 20 header + 2 + 2 axes + 4 × 4 values + 4 classes + 4 region = 48.
        assert_eq!(bytes.len(), 48 * 4);
        assert_eq!(&bytes[0..4], b"PECM");
        assert_eq!(
            (1..10).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [2, 2, 2, 1, 1, 1, 0, 2, 2]
        );
        assert_eq!(float(&bytes, 10), 0.05);
        assert_eq!(
            (11..19).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [0.5, 0.5, 0.5, 0.5, 7.142857, 7.142857, 7.142857, 7.142857]
        );
        assert_eq!(word(&bytes, 19), 3);
        assert_eq!(
            (20..24).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [0.0, 90.0, 6.0, 12.0]
        );
        // A 24–27, B 28–31, Δ kn 32–35, Δ % 36–39.
        assert_eq!(
            (24..28).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [0.0, 0.0, 7.5, 8.0]
        );
        assert!(float(&bytes, 29).is_nan() && float(&bytes, 31).is_nan());
        assert_eq!(float(&bytes, 34), 0.5);
        assert!(float(&bytes, 32).is_nan());
        assert_eq!(float(&bytes, 38), 7.142857);
        // Classes 40–43, the region 44–47.
        assert_eq!(
            (40..48).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [4, 4, 3, 1, 0, 1, 1, 1]
        );
    }

    /// The same bytes the frontend's unpacking test reads. `PE_BLESS=1`
    /// rewrites the file after a deliberate layout change (and the version
    /// must change with it).
    #[test]
    fn the_frontend_fixture_holds_these_bytes() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/compare/fixtures");
        let bytes = pack(&fixture());
        let path = dir.join("compare-v2.bin");
        if std::env::var_os("PE_BLESS").is_some() {
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn an_empty_comparison_is_a_header_with_no_statistics() {
        let c = pe_polar::compare(&Polar::default(), &Polar::default(), 0.05).unwrap();
        let bytes = pack(&c);
        assert_eq!(bytes.len(), HEADER_BYTES);
        assert_eq!(word(&bytes, 8), NO_CELL);
        assert!(float(&bytes, 11).is_nan());
    }
}
