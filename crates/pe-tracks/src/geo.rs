//! Spherical geodesy on the ERA5 Earth (radius 6,371,229 m, CLAUDE.md).
//!
//! Every function takes and returns degrees, is exact across the
//! antimeridian (longitudes are only ever differenced through trigonometry,
//! never subtracted and used raw) and is defined at the poles.

/// Earth's radius, metres: the ERA5 and GRIB shape of earth.
pub const EARTH_RADIUS_M: f64 = 6_371_229.0;
/// One knot in metres per second.
pub const KNOT_M_S: f64 = 1852.0 / 3600.0;
/// Below this distance two positions are the same place and have no
/// bearing between them, metres.
pub const SAME_PLACE_M: f64 = 1e-6;

/// Great-circle distance, metres (haversine, stable for short distances).
pub fn distance_m(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let dphi = phi2 - phi1;
    let dlam = (lon2 - lon1).to_radians();
    let a = (dphi / 2.0).sin().powi(2) + phi1.cos() * phi2.cos() * (dlam / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * a.sqrt().atan2((1.0 - a).max(0.0).sqrt())
}

/// Initial great-circle bearing from the first position to the second,
/// degrees in [0, 360). `None` when they are the same place, where no
/// direction exists.
pub fn initial_bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> Option<f64> {
    if distance_m(lat1, lon1, lat2, lon2) < SAME_PLACE_M {
        return None;
    }
    let (phi1, phi2) = (lat1.to_radians(), lat2.to_radians());
    let dlam = (lon2 - lon1).to_radians();
    let y = dlam.sin() * phi2.cos();
    let x = phi1.cos() * phi2.sin() - phi1.sin() * phi2.cos() * dlam.cos();
    Some(wrap_360(y.atan2(x).to_degrees()))
}

/// Final great-circle bearing on arriving at the second position from the
/// first, degrees in [0, 360): the direction of travel *at* the second
/// position. `None` when they are the same place.
pub fn final_bearing_deg(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> Option<f64> {
    initial_bearing_deg(lat2, lon2, lat1, lon1).map(|b| wrap_360(b + 180.0))
}

/// The circular mean of two directions, degrees in [0, 360); `None` when
/// they point opposite ways and have no mean.
pub fn mean_direction(a: f64, b: f64) -> Option<f64> {
    let (a, b) = (a.to_radians(), b.to_radians());
    let (y, x) = (a.sin() + b.sin(), a.cos() + b.cos());
    (y.hypot(x) > 1e-9).then(|| wrap_360(y.atan2(x).to_degrees()))
}

/// An angle folded into [0, 360). One already there is returned
/// unchanged, bit for bit: imported values are kept exactly (invariant 1).
pub fn wrap_360(degrees: f64) -> f64 {
    if (0.0..360.0).contains(&degrees) {
        return degrees;
    }
    let wrapped = degrees.rem_euclid(360.0);
    // rem_euclid can round up to exactly 360 for a tiny negative input.
    if wrapped >= 360.0 { 0.0 } else { wrapped }
}

/// A longitude folded into [-180, 180); one already there is returned
/// unchanged, bit for bit.
pub fn wrap_lon(lon: f64) -> f64 {
    if (-180.0..180.0).contains(&lon) {
        lon
    } else {
        wrap_360(lon + 180.0) - 180.0
    }
}

/// The smaller angle between two directions, degrees in [0, 180].
pub fn angle_between(a: f64, b: f64) -> f64 {
    let d = wrap_360(a - b);
    if d > 180.0 { 360.0 - d } else { d }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol
    }

    /// One degree of arc on this sphere is R·π/180 = 111,198.92 m.
    const DEGREE_M: f64 = EARTH_RADIUS_M * std::f64::consts::PI / 180.0;

    #[test]
    fn a_degree_of_latitude_or_of_equator_is_r_pi_over_180() {
        assert!(close(DEGREE_M, 111_198.92, 0.01));
        assert!(close(distance_m(0.0, 0.0, 1.0, 0.0), DEGREE_M, 1e-6));
        assert!(close(distance_m(0.0, 0.0, 0.0, 1.0), DEGREE_M, 1e-6));
        assert_eq!(distance_m(50.0, -1.0, 50.0, -1.0), 0.0);
    }

    #[test]
    fn distances_and_bearings_cross_the_antimeridian() {
        // 179.5°E to 179.5°W along the equator is one degree, heading east.
        assert!(close(distance_m(0.0, 179.5, 0.0, -179.5), DEGREE_M, 1e-6));
        assert!(close(
            initial_bearing_deg(0.0, 179.5, 0.0, -179.5).unwrap(),
            90.0,
            1e-9
        ));
        assert!(close(
            initial_bearing_deg(0.0, -179.5, 0.0, 179.5).unwrap(),
            270.0,
            1e-9
        ));
        // The same longitude written as -180 and 180.
        assert!(distance_m(10.0, -180.0, 10.0, 180.0) < 1e-6);
    }

    #[test]
    fn bearings_at_and_towards_the_poles() {
        assert!(close(
            initial_bearing_deg(10.0, 30.0, 90.0, 0.0).unwrap(),
            0.0,
            1e-9
        ));
        assert!(close(
            initial_bearing_deg(-10.0, 30.0, -90.0, 0.0).unwrap(),
            180.0,
            1e-9
        ));
        // From the North Pole every way is south.
        assert!(close(
            initial_bearing_deg(90.0, 0.0, 89.0, 0.0).unwrap(),
            180.0,
            1e-9
        ));
        assert!(close(
            distance_m(89.0, 0.0, 89.0, 180.0),
            2.0 * DEGREE_M,
            1e-6
        ));
    }

    #[test]
    fn the_same_place_has_no_bearing() {
        assert_eq!(initial_bearing_deg(50.0, -1.0, 50.0, -1.0), None);
        assert_eq!(initial_bearing_deg(0.0, 180.0, 0.0, -180.0), None);
    }

    #[test]
    fn a_hand_computed_diagonal() {
        // (0,0) → (1,1): bearing atan2(sin1°·cos1°, sin1°) = 44.9956°, and
        // distance 2R·asin(√(sin²0.5° + cos1°·sin²0.5°)) = 157,255.03 m.
        let bearing = initial_bearing_deg(0.0, 0.0, 1.0, 1.0).unwrap();
        let expected = (1f64.to_radians().sin() * 1f64.to_radians().cos())
            .atan2(1f64.to_radians().sin())
            .to_degrees();
        assert!(close(bearing, expected, 1e-12));
        assert!(close(bearing, 44.9956, 1e-4));
        assert!(close(distance_m(0.0, 0.0, 1.0, 1.0), 157_255.03, 0.01));
    }

    /// Along the 60°N parallel from 0° to 10°E the great circle leaves at
    /// 85.667° and arrives at 94.333° (atan2 by hand: tan⁻¹(sin10°·cos60° /
    /// (cos60°·sin60° − sin60°·cos60°·cos10°))).
    #[test]
    fn final_bearings_and_mean_directions() {
        assert!(close(
            initial_bearing_deg(60.0, 0.0, 60.0, 10.0).unwrap(),
            85.667_126,
            1e-6
        ));
        assert!(close(
            final_bearing_deg(60.0, 0.0, 60.0, 10.0).unwrap(),
            94.332_874,
            1e-6
        ));
        assert!(close(
            final_bearing_deg(0.0, 0.0, 0.0, 1.0).unwrap(),
            90.0,
            1e-9
        ));
        assert_eq!(final_bearing_deg(1.0, 1.0, 1.0, 1.0), None);
        assert!(
            close(mean_direction(350.0, 10.0).unwrap(), 0.0, 1e-9)
                || close(mean_direction(350.0, 10.0).unwrap(), 360.0, 1e-9)
        );
        assert!(close(mean_direction(80.0, 100.0).unwrap(), 90.0, 1e-9));
        assert_eq!(mean_direction(0.0, 180.0), None);
    }

    #[test]
    fn wrapping() {
        assert_eq!(wrap_lon(180.0), -180.0);
        assert_eq!(wrap_lon(-180.0), -180.0);
        assert_eq!(wrap_lon(190.0), -170.0);
        assert_eq!(wrap_lon(-190.0), 170.0);
        assert_eq!(wrap_lon(-1.3), -1.3);
        assert_eq!(wrap_360(-1e-18), 0.0);
        assert_eq!(wrap_360(-90.0), 270.0);
        assert_eq!(angle_between(350.0, 10.0), 20.0);
        assert_eq!(angle_between(10.0, 190.0), 180.0);
    }
}
