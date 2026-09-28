//! Polar edits (spec.md 10.4): reading a source's overrides onto its grid,
//! and the values the scale and smooth tools give a selection.
//!
//! Edits are stored as `cell_overrides` beside the source (invariant 1); this
//! module only computes. A tool's result is the new boat speed of each
//! selected cell, which the app records as one undoable command.

use pe_core::source::{CellOverride, MAX_EDIT_BSP_KN, Overlay};

use crate::Polar;

/// How close to an axis value counts as on it; axes are canonical, so this
/// only absorbs arithmetic noise.
const ON_AXIS: f64 = 1e-9;

/// The index of `value` on `axis`, if it is one of its values.
pub fn axis_index(axis: &[f64], value: f64) -> Option<usize> {
    axis.iter().position(|v| (v - value).abs() <= ON_AXIS)
}

/// `grid` with every override that names one of its nodes written in.
/// An override off the grid (the output grid has since changed) is kept in
/// the overlay but has nothing to apply to.
pub fn apply_overrides(grid: &mut Polar, overrides: &[CellOverride]) {
    for edit in overrides {
        let (Some(i), Some(j)) = (
            axis_index(&grid.twa, edit.twa),
            axis_index(&grid.tws, edit.tws),
        ) else {
            continue;
        };
        if let Some(cell) = grid.bsp.get_mut(i).and_then(|row| row.get_mut(j)) {
            *cell = Some(edit.bsp);
        }
    }
}

/// Whether the node at row `i`, column `j` of `grid` has an override.
pub fn is_overridden(grid: &Polar, overlay: &Overlay, i: usize, j: usize) -> bool {
    match (grid.twa.get(i), grid.tws.get(j)) {
        (Some(twa), Some(tws)) => overlay.override_at(*twa, *tws).is_some(),
        _ => false,
    }
}

/// Keeps a tool's answer a speed a cell can hold.
fn clamp(bsp: f64) -> f64 {
    bsp.clamp(0.0, MAX_EDIT_BSP_KN)
}

/// Each selected cell with a value, scaled by `percent` (+10 is 10 %
/// faster). Empty cells have nothing to scale and are left out.
pub fn scaled(grid: &Polar, cells: &[(usize, usize)], percent: f64) -> Vec<((usize, usize), f64)> {
    let factor = 1.0 + percent / 100.0;
    cells
        .iter()
        .filter_map(|&(i, j)| grid.get(i, j).map(|bsp| ((i, j), clamp(bsp * factor))))
        .collect()
}

/// The 3×3 binomial kernel: 4 at the centre, 2 beside, 1 at the corners.
const KERNEL: [[f64; 3]; 3] = [[1.0, 2.0, 1.0], [2.0, 4.0, 2.0], [1.0, 2.0, 1.0]];

/// Each selected cell with a value, smoothed over its 3×3 neighbourhood on
/// the grid (spec.md 10.4) with the binomial kernel, weighted over the
/// neighbours that have a value. Every cell reads the grid as it was, so
/// the order of the selection does not matter. Empty cells stay empty: a
/// hole is not filled by smoothing.
pub fn smoothed(grid: &Polar, cells: &[(usize, usize)]) -> Vec<((usize, usize), f64)> {
    cells
        .iter()
        .filter_map(|&(i, j)| {
            grid.get(i, j)?;
            let mut sum = 0.0;
            let mut weight = 0.0;
            for (di, row) in KERNEL.iter().enumerate() {
                for (dj, w) in row.iter().enumerate() {
                    let (Some(ni), Some(nj)) = ((i + di).checked_sub(1), (j + dj).checked_sub(1))
                    else {
                        continue;
                    };
                    if let Some(value) = grid.get(ni, nj) {
                        sum += w * value;
                        weight += w;
                    }
                }
            }
            (weight > 0.0).then(|| ((i, j), clamp(sum / weight)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid() -> Polar {
        Polar {
            twa: vec![45.0, 90.0, 135.0],
            tws: vec![6.0, 10.0, 14.0],
            bsp: vec![
                vec![Some(4.0), Some(5.0), Some(6.0)],
                vec![Some(5.0), Some(9.0), Some(7.0)],
                vec![Some(4.5), None, Some(6.5)],
            ],
        }
    }

    #[test]
    fn overrides_on_the_grid_are_written_in_and_others_ignored() {
        let mut polar = grid();
        apply_overrides(
            &mut polar,
            &[
                CellOverride {
                    twa: 90.0,
                    tws: 10.0,
                    bsp: 6.0,
                },
                // Fills a hole: a typed value is the user's.
                CellOverride {
                    twa: 135.0,
                    tws: 10.0,
                    bsp: 6.2,
                },
                CellOverride {
                    twa: 100.0,
                    tws: 10.0,
                    bsp: 1.0,
                },
            ],
        );
        assert_eq!(polar.bsp[1][1], Some(6.0));
        assert_eq!(polar.bsp[2][1], Some(6.2));
        assert_eq!(polar.bsp[0], grid().bsp[0]);
    }

    #[test]
    fn scaling_multiplies_cells_with_values_and_skips_holes() {
        let polar = grid();
        let out = scaled(&polar, &[(0, 0), (2, 1), (1, 1)], 10.0);
        assert_eq!(out.len(), 2);
        assert!((out[0].1 - 4.4).abs() < 1e-12);
        assert!((out[1].1 - 9.9).abs() < 1e-12);
        assert_eq!(scaled(&polar, &[(1, 1)], -200.0), vec![((1, 1), 0.0)]);
    }

    /// Hand-computed. Centre (1, 1): 1·4 + 2·5 + 1·6 + 2·5 + 4·9 + 2·7 +
    /// 1·4.5 + 1·6.5 (the hole below the centre is left out, weight 2) =
    /// 91 over 14. Corner (0, 0): 4·4 + 2·5 + 2·5 + 1·9 = 45 over 9 = 5.
    #[test]
    fn smoothing_is_the_binomial_kernel_over_present_neighbours() {
        let polar = grid();
        let out = smoothed(&polar, &[(1, 1), (0, 0), (2, 1)]);
        assert_eq!(out.len(), 2, "the hole stays a hole");
        assert!((out[0].1 - 91.0 / 14.0).abs() < 1e-12);
        assert!((out[1].1 - 5.0).abs() < 1e-12);
        // A flat grid smooths to itself.
        let flat = Polar {
            twa: vec![1.0, 2.0],
            tws: vec![1.0, 2.0],
            bsp: vec![vec![Some(3.0); 2]; 2],
        };
        assert!(
            smoothed(&flat, &[(0, 0), (1, 1)])
                .iter()
                .all(|(_, v)| (v - 3.0).abs() < 1e-12)
        );
    }
}
