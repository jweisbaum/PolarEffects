//! Shape-preserving interpolation (Fritsch–Butland PCHIP). Slopes use the
//! weighted harmonic mean; one-sided endpoints are limited to avoid overshoot.
//! See the SciPy PchipInterpolator reference (linked in spec.md).

use pe_core::project::Interpolation;

use crate::{Polar, grid};

/// A prepared, strictly increasing sequence of known values. No extrapolation.
#[derive(Debug)]
pub struct Curve {
    points: Vec<(f64, f64)>,
    slopes: Vec<f64>,
}

impl Curve {
    /// Prepare a curve once for repeated reads. Empty and single-point curves
    /// are useful for ragged polar columns and need no special caller handling.
    pub fn new(points: Vec<(f64, f64)>, mode: Interpolation) -> Self {
        let n = points.len();
        let mut slopes = Vec::new();
        if mode == Interpolation::MonotoneSpline && n > 1 {
            let h: Vec<_> = points.windows(2).map(|p| p[1].0 - p[0].0).collect();
            let d: Vec<_> = points
                .windows(2)
                .zip(&h)
                .map(|(p, h)| (p[1].1 - p[0].1) / h)
                .collect();
            slopes = vec![d[0]; n];
            for i in 1..n - 1 {
                slopes[i] = if d[i - 1] * d[i] <= 0.0 {
                    0.0
                } else {
                    let (w1, w2) = (2.0 * h[i] + h[i - 1], h[i] + 2.0 * h[i - 1]);
                    (w1 + w2) / (w1 / d[i - 1] + w2 / d[i])
                };
            }
            if n > 2 {
                slopes[0] = endpoint(h[0], h[1], d[0], d[1]);
                slopes[n - 1] = endpoint(h[n - 2], h[n - 3], d[n - 2], d[n - 3]);
            }
        }
        Self { points, slopes }
    }

    /// Read between known nodes; exact nodes keep their exact values.
    pub fn at(&self, x: f64) -> Option<f64> {
        let first = self.points.first()?;
        let last = self.points.last()?;
        if !x.is_finite() || x < first.0 - 1e-9 || x > last.0 + 1e-9 {
            return None;
        }
        let hi = self.points.partition_point(|p| p.0 < x);
        for i in [hi.saturating_sub(1), hi.min(self.points.len() - 1)] {
            if (self.points[i].0 - x).abs() <= 1e-9 {
                return Some(self.points[i].1);
            }
        }
        let lo = hi.checked_sub(1)?;
        let (x0, y0) = self.points[lo];
        let (x1, y1) = *self.points.get(hi)?;
        let h = x1 - x0;
        let t = (x - x0) / h;
        if self.slopes.is_empty() {
            return Some(y0 + t * (y1 - y0));
        }
        let (t2, t3) = (t * t, t * t * t);
        let value = (2.0 * t3 - 3.0 * t2 + 1.0) * y0
            + (t3 - 2.0 * t2 + t) * h * self.slopes[lo]
            + (-2.0 * t3 + 3.0 * t2) * y1
            + (t3 - t2) * h * self.slopes[hi];
        Some(value.clamp(y0.min(y1), y0.max(y1)))
    }
}

fn endpoint(h0: f64, h1: f64, d0: f64, d1: f64) -> f64 {
    let slope = ((2.0 * h0 + h1) * d0 - h0 * d1) / (h0 + h1);
    if slope * d0 <= 0.0 {
        0.0
    } else if d0 * d1 <= 0.0 && slope.abs() > 3.0 * d0.abs() {
        3.0 * d0
    } else {
        slope
    }
}

/// Prepared polar columns. The existing bilinear coverage gate also applies
/// to splines, so spline mode never invents coverage across missing TWS cells.
#[derive(Debug)]
pub struct Interpolator<'a> {
    grid: &'a Polar,
    mode: Interpolation,
    columns: Vec<Curve>,
}

impl<'a> Interpolator<'a> {
    /// Compute column derivatives once, rather than once per output cell.
    pub fn new(grid: &'a Polar, mode: Interpolation) -> Self {
        let columns = if mode == Interpolation::Linear {
            Vec::new()
        } else {
            (0..grid.tws.len())
                .map(|j| {
                    Curve::new(
                        grid.twa
                            .iter()
                            .enumerate()
                            .filter_map(|(i, a)| Some((*a, grid.get(i, j)?)))
                            .collect(),
                        mode,
                    )
                })
                .collect()
        };
        Self {
            grid,
            mode,
            columns,
        }
    }

    /// Read a boat speed using the selected interpolation rule.
    pub fn at(&self, angle: f64, wind: f64) -> Option<f64> {
        let linear = grid::interpolate(self.grid, angle, wind)?;
        if self.mode == Interpolation::Linear {
            return Some(linear);
        }
        let angle = grid::angle_on_axis(&self.grid.twa, angle)?;
        let points = self
            .columns
            .iter()
            .zip(&self.grid.tws)
            .filter_map(|(curve, tws)| Some((*tws, curve.at(angle)?)))
            .collect();
        Curve::new(points, self.mode).at(wind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hermite_matches_hand_computed_slopes_and_never_overshoots() {
        // x=0,1,2; y=0,1,1. Endpoint slopes 1.5 and 0; middle 0.
        // At 0.5: h00*0 + h10*1.5 + h01*1 + h11*0 = 0.6875.
        let curve = Curve::new(
            vec![(0.0, 0.0), (1.0, 1.0), (2.0, 1.0)],
            Interpolation::MonotoneSpline,
        );
        assert_eq!(curve.at(0.5), Some(0.6875));
        assert_eq!(curve.at(1.5), Some(1.0));
        assert_eq!(curve.at(-0.1), None);
        assert_eq!(curve.at(2.1), None);
        let extrema = Curve::new(
            vec![(0.0, 2.0), (1.0, 8.0), (3.0, 1.0), (4.0, 1.0)],
            Interpolation::MonotoneSpline,
        );
        for k in 0..=400 {
            assert!((1.0..=8.0).contains(&extrema.at(f64::from(k) / 100.0).unwrap()));
        }
    }

    #[test]
    fn two_points_and_single_points_preserve_linear_behavior() {
        let curve = Curve::new(vec![(2.0, 4.0), (6.0, 8.0)], Interpolation::MonotoneSpline);
        assert_eq!(curve.at(3.0), Some(5.0));
        let point = Curve::new(vec![(2.0, 4.0)], Interpolation::MonotoneSpline);
        assert_eq!(point.at(2.0), Some(4.0));
        assert_eq!(point.at(3.0), None);
    }
}
