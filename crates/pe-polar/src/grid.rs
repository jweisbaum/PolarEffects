//! Reading values off a polar: folding, interpolation and resampling onto
//! another grid (spec.md 12.2).
//!
//! **Nothing is extrapolated.** A value exists between two known points and
//! nowhere else: below a column's first known angle, above its last, or
//! outside the TWS axis, the answer is "no value", and the blend (spec.md
//! 12.3) decides what fills it.
//!
//! Interpolation is bilinear in TWA × TWS, done as two linear steps: first
//! along TWA within each of the two bracketing TWS columns, then along TWS
//! between them. On a full grid that is exactly bilinear. On a ragged one — an
//! Expedition file keeps each wind speed's own angles, so its union TWA axis
//! has holes — each column is read between its own nearest known angles, which
//! is what the file says about that wind speed. A corner the query needs that
//! has no value makes the whole answer empty.

use pe_core::polar::PolarGrid;

/// How close to an axis value counts as on it. Axes are canonical to 1e-6 kn
/// and 1e-9°, so this only absorbs arithmetic noise.
const ON_AXIS: f64 = 1e-9;

/// Folds a true wind angle onto [0, 180]: port and starboard are the same
/// polar. Angles in (180, 360] become `360 − TWA`. Anything else (negative,
/// past 360, not finite) is not an angle a polar holds.
pub fn fold_twa(twa: f64) -> Option<f64> {
    if !(0.0..=360.0).contains(&twa) {
        None
    } else if twa > 180.0 {
        Some(360.0 - twa)
    } else {
        Some(twa)
    }
}

/// A full-circle grid keeps the side; a half-circle source reads symmetrically.
pub fn angle_on_axis(axis: &[f64], angle: f64) -> Option<f64> {
    if !(0.0..=360.0).contains(&angle) {
        return None;
    }
    if axis.last().is_some_and(|last| *last > 180.0) {
        Some(angle)
    } else {
        fold_twa(angle)
    }
}

/// Convert a source for the project's display mode without rewriting its data.
/// Half-circle sources seed both sides; full-circle sources average paired
/// cells only when the project explicitly selects symmetric mode.
pub fn directional(grid: &PolarGrid, asymmetric: bool) -> PolarGrid {
    let full = grid.twa.last().is_some_and(|a| *a > 180.0);
    if full == asymmetric {
        return grid.clone();
    }
    let mut axis = grid.twa.clone();
    if asymmetric {
        axis.extend(grid.twa.iter().map(|a| 360.0 - a));
    } else {
        axis.iter_mut()
            .for_each(|a| *a = fold_twa(*a).unwrap_or(*a));
    }
    axis.sort_by(f64::total_cmp);
    axis.dedup();
    let bsp = axis
        .iter()
        .map(|angle| {
            grid.tws
                .iter()
                .enumerate()
                .map(|(j, _)| {
                    // Only original nodes participate. Reading interpolated
                    // values here would silently fill ragged source holes.
                    let mut sum = 0.0;
                    let mut count = 0;
                    for (i, original) in grid.twa.iter().enumerate() {
                        let mapped = if asymmetric {
                            fold_twa(*angle)
                        } else {
                            fold_twa(*original)
                        };
                        let target = if asymmetric { *original } else { *angle };
                        if mapped.is_some_and(|a| (a - target).abs() <= ON_AXIS)
                            && let Some(value) = grid.get(i, j)
                        {
                            sum += value;
                            count += 1;
                        }
                    }
                    (count > 0).then(|| sum / f64::from(count))
                })
                .collect()
        })
        .collect();
    PolarGrid {
        twa: axis,
        tws: grid.tws.clone(),
        bsp,
    }
}

/// The number of cells holding a value.
pub fn cell_count(grid: &PolarGrid) -> usize {
    grid.bsp
        .iter()
        .flatten()
        .filter(|cell| cell.is_some())
        .count()
}

/// Where `x` falls on `axis`: the two indices either side and how far
/// between them (0 at the first). On an axis value both indices are the same.
/// `None` outside the axis — the no-extrapolation rule.
fn bracket(axis: &[f64], x: f64) -> Option<(usize, usize, f64)> {
    let first = *axis.first()?;
    let last = *axis.last()?;
    if !x.is_finite() || x < first - ON_AXIS || x > last + ON_AXIS {
        return None;
    }
    for (i, value) in axis.iter().enumerate() {
        if (x - value).abs() <= ON_AXIS {
            return Some((i, i, 0.0));
        }
    }
    let upper = axis.iter().position(|value| *value > x)?;
    let lower = upper.checked_sub(1)?;
    let t = (x - axis[lower]) / (axis[upper] - axis[lower]);
    Some((lower, upper, t))
}

/// Linear interpolation from `a` to `b`.
fn lerp(a: f64, b: f64, t: f64) -> f64 {
    a + (b - a) * t
}

/// The speed in column `j` at `twa`, from that column's own known angles,
/// and the rows it was read from (the same row twice on a known angle).
fn column_at(grid: &PolarGrid, j: usize, twa: f64) -> Option<(f64, usize, usize)> {
    let mut below: Option<(usize, f64, f64)> = None;
    let mut above: Option<(usize, f64, f64)> = None;
    for (i, angle) in grid.twa.iter().enumerate() {
        let Some(bsp) = grid.get(i, j) else { continue };
        if (angle - twa).abs() <= ON_AXIS {
            return Some((bsp, i, i));
        }
        if *angle < twa {
            below = Some((i, *angle, bsp));
        } else if above.is_none() {
            above = Some((i, *angle, bsp));
        }
    }
    let ((i0, a0, b0), (i1, a1, b1)) = (below?, above?);
    Some((lerp(b0, b1, (twa - a0) / (a1 - a0)), i0, i1))
}

/// The nodes, `(row, column)`, a value was read from: up to four corners.
pub type Corners = [(usize, usize); 4];

/// The boat speed at (`twa`, `tws`) and the nodes it was read from, or
/// `None` where the polar says nothing. What [`interpolate`] reads.
pub fn interpolate_from(grid: &PolarGrid, twa: f64, tws: f64) -> Option<(f64, Corners)> {
    let twa = angle_on_axis(&grid.twa, twa)?;
    let (j0, j1, t) = bracket(&grid.tws, tws)?;
    // Outside the TWA axis there is nothing either, whatever the column.
    bracket(&grid.twa, twa)?;
    let (low, a, b) = column_at(grid, j0, twa)?;
    if j0 == j1 {
        return Some((low, [(a, j0), (b, j0), (a, j0), (b, j0)]));
    }
    let (high, c, d) = column_at(grid, j1, twa)?;
    Some((lerp(low, high, t), [(a, j0), (b, j0), (c, j1), (d, j1)]))
}

/// The boat speed at (`twa`, `tws`), or `None` where the polar says nothing.
/// `twa` is folded first, so 220° reads the 140° value.
pub fn interpolate(grid: &PolarGrid, twa: f64, tws: f64) -> Option<f64> {
    interpolate_from(grid, twa, tws).map(|(bsp, _)| bsp)
}

/// `grid` read at every cell of the given axes (spec.md 12.2). Cells outside
/// the source's coverage are empty.
pub fn resample(grid: &PolarGrid, twa: &[f64], tws: &[f64]) -> PolarGrid {
    let bsp = twa
        .iter()
        .map(|angle| {
            tws.iter()
                .map(|speed| interpolate(grid, *angle, *speed))
                .collect()
        })
        .collect();
    PolarGrid {
        twa: twa.to_vec(),
        tws: tws.to_vec(),
        bsp,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A 3 × 2 grid with hand-picked values: rows 40°, 90°, 150°; columns
    /// 6 kn and 12 kn.
    fn square() -> PolarGrid {
        PolarGrid {
            twa: vec![40.0, 90.0, 150.0],
            tws: vec![6.0, 12.0],
            bsp: vec![
                vec![Some(4.0), Some(6.0)],
                vec![Some(6.0), Some(8.0)],
                vec![Some(5.0), Some(9.0)],
            ],
        }
    }

    fn close(a: Option<f64>, b: f64) {
        let a = a.unwrap_or(f64::NAN);
        assert!((a - b).abs() < 1e-12, "{a} != {b}");
    }

    #[test]
    fn folding_maps_port_onto_starboard() {
        assert_eq!(fold_twa(0.0), Some(0.0));
        assert_eq!(fold_twa(180.0), Some(180.0));
        assert_eq!(fold_twa(220.0), Some(140.0));
        assert_eq!(fold_twa(360.0), Some(0.0));
        assert_eq!(fold_twa(-1.0), None);
        assert_eq!(fold_twa(361.0), None);
        assert_eq!(fold_twa(f64::NAN), None);
    }

    #[test]
    fn directional_conversion_preserves_ragged_nodes_and_independent_sides() {
        let full = PolarGrid {
            twa: vec![45.0, 90.0, 270.0, 315.0],
            tws: vec![6.0, 12.0],
            bsp: vec![
                vec![Some(4.0), None],
                vec![Some(6.0), Some(8.0)],
                vec![Some(10.0), None],
                vec![None, Some(9.0)],
            ],
        };
        let half = directional(&full, false);
        assert_eq!(half.twa, vec![45.0, 90.0]);
        assert_eq!(
            half.bsp,
            vec![vec![Some(4.0), Some(9.0)], vec![Some(8.0), Some(8.0)]]
        );
        let mirrored = directional(&square(), true);
        assert_eq!(mirrored.get(4, 0), Some(6.0)); // 270°, same original 90° row.
        assert_eq!(directional(&full, true), full);
    }

    #[test]
    fn on_a_node_the_value_is_the_node() {
        let grid = square();
        close(interpolate(&grid, 90.0, 12.0), 8.0);
        close(interpolate(&grid, 40.0, 6.0), 4.0);
    }

    /// Hand-computed: at 65°, 9 kn the four corners are 4, 6 (40°) and 6, 8
    /// (90°). Along TWS, halfway: 5 and 7. Along TWA, halfway: 6.
    #[test]
    fn between_nodes_it_is_bilinear() {
        let grid = square();
        close(interpolate(&grid, 65.0, 9.0), 6.0);
        // A quarter of the way from 90° to 150° at 12 kn: 8 + 0.25 × 1.
        close(interpolate(&grid, 105.0, 12.0), 8.25);
        // Port reads starboard.
        close(interpolate(&grid, 295.0, 9.0), 6.0);
    }

    #[test]
    fn nothing_is_extrapolated() {
        let grid = square();
        assert_eq!(interpolate(&grid, 30.0, 9.0), None);
        assert_eq!(interpolate(&grid, 170.0, 9.0), None);
        assert_eq!(interpolate(&grid, 90.0, 4.0), None);
        assert_eq!(interpolate(&grid, 90.0, 14.0), None);
        assert_eq!(interpolate(&PolarGrid::default(), 90.0, 10.0), None);
    }

    #[test]
    fn a_missing_corner_empties_the_cell_it_bounds() {
        let mut grid = square();
        grid.bsp[1][1] = None;
        // The 12 kn column still spans 40°–150° by its own known angles, so
        // it reads across the hole.
        close(interpolate(&grid, 65.0, 12.0), 6.0 + 3.0 * 25.0 / 110.0);
        grid.bsp[2][1] = None;
        assert_eq!(interpolate(&grid, 65.0, 9.0), None);
        assert_eq!(interpolate(&grid, 65.0, 12.0), None);
        close(interpolate(&grid, 65.0, 6.0), 5.0);
    }

    /// Two wind speeds with their own beat angles, as an Expedition file
    /// gives them: each column is read between its own points.
    #[test]
    fn ragged_columns_read_between_their_own_points() {
        let grid = PolarGrid {
            twa: vec![40.0, 45.0, 90.0],
            tws: vec![6.0, 12.0],
            bsp: vec![
                vec![None, Some(6.0)],
                vec![Some(5.0), None],
                vec![Some(7.0), Some(9.0)],
            ],
        };
        // 6 kn: 45° → 90°, 5 → 7. At 67.5°: 6.
        // 12 kn: 40° → 90°, 6 → 9. At 67.5°: 6 + 3 × 0.55 = 7.65.
        close(interpolate(&grid, 67.5, 6.0), 6.0);
        close(interpolate(&grid, 67.5, 12.0), 7.65);
        close(interpolate(&grid, 67.5, 9.0), (6.0 + 7.65) / 2.0);
        // 42° is below 6 kn's first angle: nothing, whatever 12 kn says.
        assert_eq!(interpolate(&grid, 42.0, 6.0), None);
        assert_eq!(interpolate(&grid, 42.0, 9.0), None);
        close(interpolate(&grid, 42.0, 12.0), 6.0 + 3.0 * 0.04);
    }

    #[test]
    fn resampling_fills_covered_cells_and_leaves_the_rest_empty() {
        let grid = square();
        let out = resample(&grid, &[0.0, 40.0, 65.0, 180.0], &[4.0, 6.0, 9.0]);
        out.validate().unwrap();
        assert_eq!(out.bsp[0], vec![None, None, None]);
        assert_eq!(out.bsp[1], vec![None, Some(4.0), Some(5.0)]);
        assert_eq!(out.bsp[2], vec![None, Some(5.0), Some(6.0)]);
        assert_eq!(out.bsp[3], vec![None, None, None]);
        assert_eq!(cell_count(&out), 4);
        // Resampling onto its own axes gives the polar back.
        assert_eq!(resample(&grid, &grid.twa, &grid.tws), grid);
    }
}
