//! Comparing two polars cell by cell (spec.md 11).
//!
//! Both operands are read onto the same axes first (the project output
//! grid, by the caller), so a comparison is a plain walk over their cells.
//! **Only cells where both have a value are compared**: a cell only one of
//! them covers is counted but has no difference, since a difference against
//! nothing would be a number invented by the absent side.
//!
//! The 0° row is 0 kn by definition (spec.md 12.3), not a measurement: it
//! takes no part in the overlap, the statistics or the regions, whatever
//! either operand holds there.
//!
//! Every difference is rounded to the canonical knot precision (1e-6 kn),
//! so the threshold test that splits "A faster" from "B faster" regions
//! gives the same answer on every platform, and a difference of exactly the
//! threshold (7.05 − 7.00) is not counted as over it.

use pe_core::canonical;
use thiserror::Error;

use crate::Polar;

/// How close to 0° counts as the 0° row.
const ON_AXIS: f64 = 1e-9;

/// The default difference, knots, below which neither side is faster
/// (spec.md 11: a Compare setting).
pub const DEFAULT_THRESHOLD_KN: f64 = 0.05;

/// The least speed of B, knots, a difference is given as a percentage of
/// (controller ruling, D28): below it the ratio says more about B's
/// rounding than about either boat — 0.02 kn against 0.01 kn is 100 % —
/// so the cell is compared in knots but "not comparable in %".
pub const MIN_PERCENT_BASE_KN: f64 = 0.1;

/// Who covers a cell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CellClass {
    /// Neither operand has a value.
    Neither,
    /// Only A has a value: drawn grey with a pattern and counted.
    AOnly,
    /// Only B has a value.
    BOnly,
    /// Both have a value: the cell is compared.
    Both,
    /// The 0° row, never compared.
    ZeroRow,
}

/// Which operand is faster over a region.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Faster {
    /// Δ = A − B above the threshold.
    A,
    /// Δ below minus the threshold.
    B,
}

/// A run of neighbouring TWA cells at one TWS where the same operand is
/// faster by more than the threshold (spec.md 11).
#[derive(Debug, Clone, PartialEq)]
pub struct Region {
    /// The wind speed column.
    pub tws_index: usize,
    /// The first TWA row of the run.
    pub first_twa_index: usize,
    /// The last TWA row of the run (inclusive).
    pub last_twa_index: usize,
    /// Which operand is faster.
    pub faster: Faster,
}

/// A statistic of |Δ| and the signed range of Δ over the compared cells.
#[derive(Debug, Clone, Copy, PartialEq, Default)]
pub struct DeltaStats {
    /// Mean |Δ|; `None` with no compared cell.
    pub mean_abs: Option<f64>,
    /// The largest |Δ| and its cell (`twa index`, `tws index`).
    pub max_abs: Option<(f64, usize, usize)>,
    /// The smallest and the largest signed Δ.
    pub range: Option<(f64, f64)>,
}

/// The comparison of A and B on common axes.
#[derive(Debug, Clone, PartialEq)]
pub struct Comparison {
    /// TWA axis, degrees.
    pub twa: Vec<f64>,
    /// TWS axis, knots.
    pub tws: Vec<f64>,
    /// A per cell, `[twa][tws]`, knots.
    pub a: Vec<Vec<Option<f64>>>,
    /// B per cell, knots.
    pub b: Vec<Vec<Option<f64>>>,
    /// Δ = A − B, knots, where both have a value (off the 0° row).
    pub delta_kn: Vec<Vec<Option<f64>>>,
    /// Δ as percent of B, where both have a value and B is at least
    /// [`MIN_PERCENT_BASE_KN`].
    pub delta_pct: Vec<Vec<Option<f64>>>,
    /// Who covers each cell.
    pub class: Vec<Vec<CellClass>>,
    /// Cells both cover.
    pub overlap: usize,
    /// Cells only A covers.
    pub a_only: usize,
    /// Cells only B covers.
    pub b_only: usize,
    /// Statistics of Δ in knots.
    pub kn: DeltaStats,
    /// Statistics of Δ in percent of B, over the cells that have one.
    pub pct: DeltaStats,
    /// Compared cells with no percentage: B below [`MIN_PERCENT_BASE_KN`].
    pub pct_excluded: usize,
    /// The threshold the regions were found with, knots.
    pub threshold_kn: f64,
    /// Regions where A or B is faster, by TWS column then TWA.
    pub regions: Vec<Region>,
}

/// Why two polars cannot be compared.
#[derive(Debug, Clone, PartialEq, Error)]
pub enum CompareError {
    /// The operands are not on the same axes.
    #[error("the two polars are not on the same axes")]
    Axes,
    /// The threshold is negative or not a number.
    #[error("the threshold {0} kn is not a speed of zero or more")]
    Threshold(f64),
}

fn is_zero_row(twa: f64) -> bool {
    twa.abs() <= ON_AXIS
}

fn cell(grid: &Polar, i: usize, j: usize) -> Option<f64> {
    grid.bsp
        .get(i)
        .and_then(|row| row.get(j))
        .copied()
        .flatten()
        .filter(|v| v.is_finite())
}

fn stats(values: &[Vec<Option<f64>>]) -> DeltaStats {
    let mut sum = 0.0;
    let mut n = 0usize;
    let mut out = DeltaStats::default();
    for (i, row) in values.iter().enumerate() {
        for (j, value) in row.iter().enumerate() {
            let Some(d) = value else { continue };
            sum += d.abs();
            n += 1;
            if out.max_abs.is_none_or(|(m, ..)| d.abs() > m) {
                out.max_abs = Some((d.abs(), i, j));
            }
            out.range = Some(match out.range {
                None => (*d, *d),
                Some((lo, hi)) => (lo.min(*d), hi.max(*d)),
            });
        }
    }
    if n > 0 {
        out.mean_abs = Some(sum / n as f64);
    }
    out
}

/// Compares `a` with `b`, which must share their axes; regions are the runs
/// where |Δ| exceeds `threshold_kn`.
pub fn compare(a: &Polar, b: &Polar, threshold_kn: f64) -> Result<Comparison, CompareError> {
    if a.twa != b.twa || a.tws != b.tws {
        return Err(CompareError::Axes);
    }
    if !(threshold_kn >= 0.0 && threshold_kn.is_finite()) {
        return Err(CompareError::Threshold(threshold_kn));
    }
    let (ni, nj) = (a.twa.len(), a.tws.len());
    let mut out = Comparison {
        twa: a.twa.clone(),
        tws: a.tws.clone(),
        a: vec![vec![None; nj]; ni],
        b: vec![vec![None; nj]; ni],
        delta_kn: vec![vec![None; nj]; ni],
        delta_pct: vec![vec![None; nj]; ni],
        class: vec![vec![CellClass::Neither; nj]; ni],
        overlap: 0,
        a_only: 0,
        b_only: 0,
        kn: DeltaStats::default(),
        pct: DeltaStats::default(),
        pct_excluded: 0,
        threshold_kn,
        regions: Vec::new(),
    };
    for i in 0..ni {
        let zero = is_zero_row(a.twa[i]);
        for j in 0..nj {
            let (va, vb) = (cell(a, i, j), cell(b, i, j));
            out.a[i][j] = va;
            out.b[i][j] = vb;
            out.class[i][j] = match (zero, va, vb) {
                (true, ..) => CellClass::ZeroRow,
                (false, Some(va), Some(vb)) => {
                    out.overlap += 1;
                    out.delta_kn[i][j] = Some(canonical::knots(va - vb));
                    if vb < MIN_PERCENT_BASE_KN {
                        out.pct_excluded += 1;
                    } else {
                        out.delta_pct[i][j] = Some(canonical::round(
                            (va - vb) / vb * 100.0,
                            canonical::KNOT_PLACES,
                        ));
                    }
                    CellClass::Both
                }
                (false, Some(_), None) => {
                    out.a_only += 1;
                    CellClass::AOnly
                }
                (false, None, Some(_)) => {
                    out.b_only += 1;
                    CellClass::BOnly
                }
                (false, None, None) => CellClass::Neither,
            };
        }
    }
    out.kn = stats(&out.delta_kn);
    out.pct = stats(&out.delta_pct);
    out.regions = regions(&out.delta_kn, ni, nj, threshold_kn);
    Ok(out)
}

/// Per TWS column, the runs of neighbouring TWA rows faster on one side.
/// A cell not compared, or within the threshold, ends a run.
fn regions(delta: &[Vec<Option<f64>>], ni: usize, nj: usize, threshold: f64) -> Vec<Region> {
    let side = |i: usize, j: usize| -> Option<Faster> {
        let d = delta[i][j]?;
        if d > threshold {
            Some(Faster::A)
        } else if d < -threshold {
            Some(Faster::B)
        } else {
            None
        }
    };
    let mut out = Vec::new();
    for j in 0..nj {
        let mut run: Option<(usize, Faster)> = None;
        for i in 0..=ni {
            let here = if i < ni { side(i, j) } else { None };
            if let Some((start, faster)) = run
                && here != Some(faster)
            {
                out.push(Region {
                    tws_index: j,
                    first_twa_index: start,
                    last_twa_index: i - 1,
                    faster,
                });
                run = None;
            }
            if run.is_none()
                && let Some(faster) = here
            {
                run = Some((i, faster));
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn grid(twa: &[f64], tws: &[f64], rows: &[&[Option<f64>]]) -> Polar {
        Polar {
            twa: twa.to_vec(),
            tws: tws.to_vec(),
            bsp: rows.iter().map(|r| r.to_vec()).collect(),
        }
    }

    /// Values worked by hand: rows 0°, 45°, 90°, 135°, columns 8 and 12 kn.
    #[test]
    fn a_small_comparison_matches_the_hand_computed_one() {
        let twa = [0.0, 45.0, 90.0, 135.0];
        let tws = [8.0, 12.0];
        let a = grid(
            &twa,
            &tws,
            &[
                &[Some(0.0), Some(0.0)],
                &[Some(6.0), Some(7.0)],
                &[Some(7.5), None],
                &[Some(7.0), Some(8.0)],
            ],
        );
        let b = grid(
            &twa,
            &tws,
            &[
                &[Some(0.0), Some(1.0)],
                &[Some(5.0), Some(7.03)],
                &[Some(8.0), Some(9.0)],
                &[None, Some(10.0)],
            ],
        );
        let c = compare(&a, &b, 0.05).unwrap();
        // 45/8: +1.0 (20 %); 45/12: −0.03 (within); 90/8: −0.5 (−6.25 %);
        // 135/12: −2.0 (−20 %). 90/12 B only; 135/8 A only.
        assert_eq!(c.overlap, 4);
        assert_eq!((c.a_only, c.b_only), (1, 1));
        assert_eq!(c.class[0][1], CellClass::ZeroRow, "0° is never compared");
        assert_eq!(c.delta_kn[1][0], Some(1.0));
        assert_eq!(c.delta_kn[1][1], Some(-0.03));
        assert_eq!(c.delta_pct[1][0], Some(20.0));
        assert_eq!(c.delta_pct[2][0], Some(-6.25));
        assert_eq!(c.delta_pct[3][1], Some(-20.0));
        // Mean |Δ| = (1 + 0.03 + 0.5 + 2) / 4 = 0.8825.
        assert!((c.kn.mean_abs.unwrap() - 0.8825).abs() < 1e-12);
        assert_eq!(c.kn.max_abs, Some((2.0, 3, 1)));
        assert_eq!(c.kn.range, Some((-2.0, 1.0)));
        assert_eq!(
            c.pct.max_abs,
            Some((20.0, 1, 0)),
            "the first of equal maxima"
        );
        assert_eq!(
            c.regions,
            vec![
                Region {
                    tws_index: 0,
                    first_twa_index: 1,
                    last_twa_index: 1,
                    faster: Faster::A
                },
                Region {
                    tws_index: 0,
                    first_twa_index: 2,
                    last_twa_index: 2,
                    faster: Faster::B
                },
                Region {
                    tws_index: 1,
                    first_twa_index: 3,
                    last_twa_index: 3,
                    faster: Faster::B
                },
            ]
        );
    }

    /// A difference of exactly the threshold is not over it, whatever the
    /// floating-point subtraction gives.
    #[test]
    fn a_difference_equal_to_the_threshold_is_no_region() {
        let a = grid(&[90.0], &[10.0], &[&[Some(7.05)]]);
        let b = grid(&[90.0], &[10.0], &[&[Some(7.0)]]);
        let c = compare(&a, &b, 0.05).unwrap();
        assert_eq!(c.delta_kn[0][0], Some(0.05));
        assert!(c.regions.is_empty());
        assert_eq!(compare(&a, &b, 0.04).unwrap().regions.len(), 1);
    }

    /// Neighbouring faster cells make one run; a gap or a within-threshold
    /// cell breaks it.
    #[test]
    fn runs_join_neighbours_and_break_on_gaps() {
        let twa = [40.0, 50.0, 60.0, 70.0, 80.0, 90.0];
        let a = grid(
            &twa,
            &[10.0],
            &[
                &[Some(6.2)],
                &[Some(6.4)],
                &[None],
                &[Some(7.3)],
                &[Some(7.2)],
                &[Some(7.0)],
            ],
        );
        let b = grid(
            &twa,
            &[10.0],
            &[
                &[Some(6.0)],
                &[Some(6.0)],
                &[Some(6.9)],
                &[Some(7.0)],
                &[Some(7.0)],
                &[Some(7.0)],
            ],
        );
        let c = compare(&a, &b, 0.05).unwrap();
        let spans: Vec<_> = c
            .regions
            .iter()
            .map(|r| (r.first_twa_index, r.last_twa_index, r.faster))
            .collect();
        assert_eq!(spans, vec![(0, 1, Faster::A), (3, 4, Faster::A)]);
    }

    /// A zero B has a difference in knots but none in percent.
    #[test]
    fn a_b_under_a_tenth_of_a_knot_has_no_percentage() {
        // B 0.0, 0.05 and 0.1 kn: the first two are compared in knots only
        // and counted; 0.1 kn is the least base, (0.3 − 0.1) / 0.1 = 200 %.
        let a = grid(
            &[60.0, 90.0, 120.0],
            &[10.0],
            &[&[Some(1.0)], &[Some(0.2)], &[Some(0.3)]],
        );
        let b = grid(
            &[60.0, 90.0, 120.0],
            &[10.0],
            &[&[Some(0.0)], &[Some(0.05)], &[Some(0.1)]],
        );
        let c = compare(&a, &b, 0.05).unwrap();
        assert_eq!(c.delta_kn[0][0], Some(1.0));
        assert_eq!(c.delta_kn[1][0], Some(0.15));
        assert_eq!(c.delta_pct[0][0], None);
        assert_eq!(c.delta_pct[1][0], None);
        assert_eq!(c.delta_pct[2][0], Some(200.0));
        assert_eq!(c.pct_excluded, 2);
        assert_eq!(c.overlap, 3);
        // Only the one comparable cell is in the percentage statistics.
        assert_eq!(c.pct.mean_abs, Some(200.0));
        assert_eq!(c.pct.max_abs, Some((200.0, 2, 0)));
    }

    #[test]
    fn different_axes_and_bad_thresholds_are_refused() {
        let a = grid(&[90.0], &[10.0], &[&[Some(1.0)]]);
        let b = grid(&[90.0], &[12.0], &[&[Some(1.0)]]);
        assert_eq!(compare(&a, &b, 0.05), Err(CompareError::Axes));
        assert!(matches!(
            compare(&a, &a, -1.0),
            Err(CompareError::Threshold(_))
        ));
        assert!(matches!(
            compare(&a, &a, f64::NAN),
            Err(CompareError::Threshold(_))
        ));
    }

    /// Comparing a polar with itself: every shared cell 0, no region.
    #[test]
    fn a_polar_equals_itself() {
        let a = grid(
            &[0.0, 60.0, 120.0],
            &[6.0, 12.0],
            &[
                &[Some(0.0), Some(0.0)],
                &[Some(5.5), Some(7.1)],
                &[None, Some(8.0)],
            ],
        );
        let c = compare(&a, &a, 0.0).unwrap();
        assert_eq!(c.overlap, 3);
        assert_eq!(c.kn.max_abs.map(|m| m.0), Some(0.0));
        assert!(c.regions.is_empty());
    }
}
