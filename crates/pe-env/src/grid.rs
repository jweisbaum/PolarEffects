//! Regular latitude/longitude grids and the bilinear stencil (spec.md 7.5).
//!
//! Each archive publishes its own grid: ERA5 (WeatherBench2 and ARCO) runs
//! north to south from +90 and east from 0; Copernicus Marine runs south to
//! north from −80 and east from −180. Nothing here assumes either; the axes
//! are read from the store and checked to be regular.

use crate::error::{EnvError, Result};

/// A regularly spaced coordinate axis.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Axis {
    /// The first value.
    pub first: f64,
    /// The spacing; negative for a descending axis.
    pub step: f64,
    /// Number of values.
    pub len: usize,
}

impl Axis {
    /// Builds an axis from its values, checking that they are evenly spaced.
    ///
    /// # Errors
    /// [`EnvError::Layout`] for fewer than two values or uneven spacing. A
    /// grid that is not regular would be sampled at the wrong place without
    /// any sign of it.
    pub fn from_values(name: &str, values: &[f64]) -> Result<Self> {
        let (Some(&first), Some(&last)) = (values.first(), values.last()) else {
            return Err(EnvError::Layout(format!("the {name} axis is empty")));
        };
        if values.len() < 2 {
            return Err(EnvError::Layout(format!(
                "the {name} axis has a single value"
            )));
        }
        let step = (last - first) / (values.len() - 1) as f64;
        if step == 0.0 || !step.is_finite() {
            return Err(EnvError::Layout(format!(
                "the {name} axis does not advance"
            )));
        }
        // Stored as float32, so allow float32 rounding at these magnitudes.
        let tolerance = step.abs() * 1e-3 + 1e-4;
        for (i, &v) in values.iter().enumerate() {
            let expected = first + step * i as f64;
            if (v - expected).abs() > tolerance {
                return Err(EnvError::Layout(format!(
                    "the {name} axis is not evenly spaced: value {i} is {v}, expected {expected}"
                )));
            }
        }
        Ok(Self {
            first,
            step,
            len: values.len(),
        })
    }

    /// The value at index `i`.
    pub fn value(&self, i: usize) -> f64 {
        self.first + self.step * i as f64
    }
}

/// The four grid points around a position and the weights between them.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Stencil {
    /// Latitude indices, the second one step further along the axis.
    pub lat: [usize; 2],
    /// Longitude indices; the second may wrap to 0 on a global grid.
    pub lon: [usize; 2],
    /// Weight of `lat[1]`, in [0, 1].
    pub lat_frac: f64,
    /// Weight of `lon[1]`, in [0, 1].
    pub lon_frac: f64,
}

impl Stencil {
    /// Bilinear interpolation over the four corner values, in the order
    /// `(lat[0], lon[0]), (lat[0], lon[1]), (lat[1], lon[0]), (lat[1], lon[1])`.
    ///
    /// Missing corners (NaN, i.e. land for waves and currents) are left out
    /// and the remaining weights renormalised; if every corner with weight is
    /// missing the value is missing (spec.md 7.5).
    pub fn interpolate(&self, corners: [f32; 4]) -> Option<f64> {
        let (a, b) = (self.lat_frac, self.lon_frac);
        let weights = [(1.0 - a) * (1.0 - b), (1.0 - a) * b, a * (1.0 - b), a * b];
        let (mut sum, mut total) = (0.0, 0.0);
        for (value, weight) in corners.iter().zip(weights) {
            if value.is_finite() && weight > 0.0 {
                sum += f64::from(*value) * weight;
                total += weight;
            }
        }
        // A position exactly on a missing node with every neighbour at zero
        // weight is missing, not the neighbour's value.
        (total > 1e-12).then(|| sum / total)
    }
}

/// A regular latitude/longitude grid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Grid {
    /// Latitude axis.
    pub lat: Axis,
    /// Longitude axis.
    pub lon: Axis,
}

impl Grid {
    /// Whether the longitude axis goes all the way round, so the cell east of
    /// the last column is the first column.
    pub fn is_global(&self) -> bool {
        (self.lon.step.abs() * self.lon.len as f64 - 360.0).abs() < self.lon.step.abs() * 0.5
    }

    /// The stencil around `(lat, lon)`, degrees, any longitude convention.
    /// `None` outside the grid.
    pub fn stencil(&self, lat: f64, lon: f64) -> Option<Stencil> {
        if !lat.is_finite() || !lon.is_finite() {
            return None;
        }
        let (lat_i, lat_frac) = along(&self.lat, lat)?;
        let lat1 = (lat_i + 1).min(self.lat.len - 1);

        let (lon0, lon_frac, lon1) = if self.is_global() {
            // A westward axis is never published; refuse rather than index
            // it backwards.
            if self.lon.step < 0.0 {
                return None;
            }
            // Longitude measured eastward from the first column, so -5 and
            // 355 land on the same cell whatever the archive's convention.
            let p = (lon - self.lon.first).rem_euclid(360.0) / self.lon.step;
            let j = (p.floor() as usize) % self.lon.len;
            (j, p - p.floor(), (j + 1) % self.lon.len)
        } else {
            let lon = normalise_near(lon, self.lon.first, self.lon.value(self.lon.len - 1));
            let (j, frac) = along(&self.lon, lon)?;
            (j, frac, (j + 1).min(self.lon.len - 1))
        };
        Some(Stencil {
            lat: [lat_i, lat1],
            lon: [lon0, lon1],
            lat_frac: if lat1 == lat_i { 0.0 } else { lat_frac },
            lon_frac: if lon1 == lon0 { 0.0 } else { lon_frac },
        })
    }
}

/// Index and fraction of `x` along `axis`; `None` outside it.
fn along(axis: &Axis, x: f64) -> Option<(usize, f64)> {
    let p = (x - axis.first) / axis.step;
    let last = (axis.len - 1) as f64;
    // A hair outside the end (float32 axis values) still counts as the end.
    if !(-1e-9..=last + 1e-9).contains(&p) {
        return None;
    }
    let p = p.clamp(0.0, last);
    let i = (p.floor() as usize).min(axis.len - 1);
    Some((i, p - i as f64))
}

/// Shifts `lon` by whole turns into the range of a regional axis if it can
/// be; otherwise leaves it (and it will be refused as outside).
fn normalise_near(lon: f64, a: f64, b: f64) -> f64 {
    let (lo, hi) = if a <= b { (a, b) } else { (b, a) };
    for shift in [0.0, 360.0, -360.0] {
        let x = lon + shift;
        if (lo..=hi).contains(&x) {
            return x;
        }
    }
    lon
}

#[cfg(test)]
mod tests {
    use super::*;

    fn era5() -> Grid {
        Grid {
            lat: Axis {
                first: 90.0,
                step: -0.25,
                len: 721,
            },
            lon: Axis {
                first: 0.0,
                step: 0.25,
                len: 1440,
            },
        }
    }

    fn cmems() -> Grid {
        Grid {
            lat: Axis {
                first: -80.0,
                step: 1.0 / 12.0,
                len: 2041,
            },
            lon: Axis {
                first: -180.0,
                step: 1.0 / 12.0,
                len: 4320,
            },
        }
    }

    #[test]
    fn an_uneven_axis_is_refused() {
        assert!(Axis::from_values("lat", &[0.0, 1.0, 3.0]).is_err());
        assert!(Axis::from_values("lat", &[1.0]).is_err());
        let a = Axis::from_values("lat", &[90.0, 89.75, 89.5]).expect("regular");
        assert_eq!(a.step, -0.25);
    }

    /// 50N 5W on the ERA5 grid, worked by hand: latitude index (90-50)/0.25 =
    /// 160 exactly; longitude 355E is column 1420 exactly.
    #[test]
    fn a_grid_node_has_zero_fractions() {
        let s = era5().stencil(50.0, -5.0).expect("inside");
        assert_eq!(s.lat, [160, 161]);
        assert_eq!(s.lon, [1420, 1421]);
        assert_eq!((s.lat_frac, s.lon_frac), (0.0, 0.0));
        assert_eq!(era5().stencil(50.0, 355.0), Some(s));
    }

    #[test]
    fn the_antimeridian_and_prime_meridian_wrap() {
        // 359.9E is between the last column (359.75) and the first (0).
        let s = era5().stencil(0.0, 359.9).expect("inside");
        assert_eq!(s.lon, [1439, 0]);
        assert!((s.lon_frac - 0.6).abs() < 1e-9);
        // On CMEMS, -180 is the first column and 179.95 wraps to it.
        let s = cmems().stencil(10.0, 179.95).expect("inside");
        assert_eq!(s.lon, [4319, 0]);
        let s = cmems().stencil(10.0, -180.0).expect("inside");
        assert_eq!(s.lon[0], 0);
        let s = cmems().stencil(10.0, 180.0).expect("same as -180");
        assert_eq!(s.lon[0], 0);
    }

    #[test]
    fn the_poles_and_outside_latitudes() {
        let s = era5().stencil(90.0, 10.0).expect("north pole row");
        assert_eq!(s.lat[0], 0);
        let s = era5().stencil(-90.0, 10.0).expect("south pole row");
        assert_eq!(s.lat, [720, 720]);
        assert_eq!(s.lat_frac, 0.0);
        // CMEMS stops at 80S.
        assert!(cmems().stencil(-85.0, 0.0).is_none());
        assert!(era5().stencil(f64::NAN, 0.0).is_none());
    }

    #[test]
    fn bilinear_weights_and_the_land_rule() {
        let s = Stencil {
            lat: [0, 1],
            lon: [0, 1],
            lat_frac: 0.25,
            lon_frac: 0.5,
        };
        // Hand-computed: (1-.25)(1-.5)*1 + (1-.25)(.5)*2 + .25*.5*3 + .25*.5*4
        //              = 0.375 + 0.75 + 0.375 + 0.5 = 2.0
        assert_eq!(s.interpolate([1.0, 2.0, 3.0, 4.0]), Some(2.0));
        // One land corner: the rest renormalised.
        let v = s
            .interpolate([f32::NAN, 2.0, 3.0, 4.0])
            .expect("three corners");
        let expected = (0.75 + 0.375 + 0.5) / (1.0 - 0.375);
        assert!((v - expected).abs() < 1e-12);
        // All land: missing.
        assert_eq!(s.interpolate([f32::NAN; 4]), None);
        // On a land node with zero weight elsewhere: missing, not a neighbour.
        let on_node = Stencil {
            lat_frac: 0.0,
            lon_frac: 0.0,
            ..s
        };
        assert_eq!(on_node.interpolate([f32::NAN, 2.0, 3.0, 4.0]), None);
    }

    #[test]
    fn a_regional_grid_accepts_either_longitude_convention() {
        let nws = Grid {
            lat: Axis {
                first: 40.0,
                step: 0.1,
                len: 251,
            },
            lon: Axis {
                first: -20.0,
                step: 0.1,
                len: 331,
            },
        };
        assert!(!nws.is_global());
        let a = nws.stencil(50.0, -5.0).expect("inside");
        let b = nws.stencil(50.0, 355.0).expect("same place");
        assert_eq!(a, b);
        assert!(nws.stencil(50.0, 20.0).is_none());
    }
}
