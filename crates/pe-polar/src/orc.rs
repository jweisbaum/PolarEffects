//! An ORC VPP as a polar (spec.md 5.3).
//!
//! The polar has the ORC angles (52°–150°) plus every beat and run angle the
//! certificate gives, on the certificate's own wind speeds (6–20, 6–24 or
//! 4–24 kn, as given). At a beat or run angle only its own wind speed has a
//! value: boat speed there is the VMG over the cosine of the angle, which is
//! what the certificate's VMG means. Nothing is interpolated or extrapolated;
//! every other cell stays empty and the blend (spec.md 12) handles the gaps.

use pe_core::orc::OrcVpp;

use crate::Polar;

/// The boat speed that makes `vmg` at `twa` degrees, if the angle is one a
/// VMG can be made good at (upwind of 90° for a beat, downwind for a run).
fn speed_from_vmg(twa: f64, vmg: f64, upwind: bool) -> Option<f64> {
    let cos = twa.to_radians().cos();
    let along = if upwind { cos } else { -cos };
    (twa.is_finite() && vmg.is_finite() && vmg >= 0.0 && along > 1e-9).then(|| vmg / along)
}

/// The polar an ORC VPP gives.
pub fn vpp_to_polar(vpp: &OrcVpp) -> Polar {
    // A beat angle past 90° or a run angle short of it makes no VMG; such an
    // angle adds no row.
    let beats = vpp.beat_angle.iter().filter(|a| **a > 0.0 && **a < 90.0);
    let runs = vpp.run_angle.iter().filter(|a| **a > 90.0 && **a <= 180.0);
    let table = vpp.angles.iter().filter(|a| (0.0..=180.0).contains(*a));
    let mut twa: Vec<f64> = table.chain(beats).chain(runs).copied().collect();
    twa.sort_by(f64::total_cmp);
    twa.dedup();
    let mut polar = Polar::empty(twa, vpp.speeds.clone());
    let row = |polar: &Polar, angle: f64| polar.twa.iter().position(|a| *a == angle);

    for (angle, speeds) in vpp.angles.iter().zip(&vpp.bsp) {
        let Some(i) = row(&polar, *angle) else {
            continue;
        };
        for (j, cell) in speeds.iter().enumerate().take(polar.tws.len()) {
            if cell.is_some_and(f64::is_finite) {
                polar.bsp[i][j] = *cell;
            }
        }
    }

    // The table's own value wins where a beat or run angle is one of its
    // angles: it is the certificate's number, the derived one is not.
    let optimum = [
        (&vpp.beat_angle, &vpp.beat_vmg, true),
        (&vpp.run_angle, &vpp.run_vmg, false),
    ];
    for (angles, vmgs, upwind) in optimum {
        for (j, (angle, vmg)) in angles.iter().zip(vmgs).enumerate().take(polar.tws.len()) {
            let (Some(i), Some(bsp)) = (row(&polar, *angle), speed_from_vmg(*angle, *vmg, upwind))
            else {
                continue;
            };
            if polar.bsp[i][j].is_none() {
                polar.bsp[i][j] = Some(bsp);
            }
        }
    }
    polar
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vpp() -> OrcVpp {
        OrcVpp {
            angles: vec![52.0, 90.0, 150.0],
            speeds: vec![6.0, 12.0],
            bsp: vec![
                vec![Some(5.9), Some(7.3)],
                vec![Some(6.8), Some(8.4)],
                vec![Some(5.1), None],
            ],
            beat_angle: vec![44.1, 39.8],
            beat_vmg: vec![4.02, 5.61],
            run_angle: vec![141.0, 150.0],
            run_vmg: vec![4.3, 7.0],
        }
    }

    #[test]
    fn the_polar_has_the_orc_angles_plus_beat_and_run_angles() {
        let polar = vpp_to_polar(&vpp());
        polar.validate().unwrap();
        assert_eq!(polar.twa, vec![39.8, 44.1, 52.0, 90.0, 141.0, 150.0]);
        assert_eq!(polar.tws, vec![6.0, 12.0]);
        // The table, exactly.
        assert_eq!(polar.bsp[2], vec![Some(5.9), Some(7.3)]);
        assert_eq!(polar.bsp[3], vec![Some(6.8), Some(8.4)]);
    }

    #[test]
    fn beat_and_run_points_come_from_the_vmg_and_nothing_is_invented() {
        let polar = vpp_to_polar(&vpp());
        // 4.02 kn made good at 44.1°: 4.02 / cos 44.1° = 5.5979 kn (by hand).
        assert!((polar.bsp[1][0].unwrap() - 5.5979).abs() < 1e-3);
        assert_eq!(
            polar.bsp[1][1], None,
            "44.1° is the beat angle at 6 kn only"
        );
        // 5.61 / cos 39.8° = 7.3029 kn.
        assert!((polar.bsp[0][1].unwrap() - 7.3029).abs() < 1e-3);
        assert_eq!(polar.bsp[0][0], None);
        // 4.3 / cos(180° − 141°) = 5.5329 kn.
        assert!((polar.bsp[4][0].unwrap() - 5.5329).abs() < 1e-3);
        // At 12 kn the run angle is 150°, where the table has no value: the
        // VMG fills it (7.0 / cos 30° = 8.0829 kn). At 6 kn the table's 5.1
        // stays.
        assert_eq!(polar.bsp[5][0], Some(5.1));
        assert!((polar.bsp[5][1].unwrap() - 8.0829).abs() < 1e-3);
        assert_eq!(crate::cell_count(&polar), 9);
    }

    #[test]
    fn impossible_optimum_angles_are_skipped() {
        let mut v = vpp();
        v.beat_angle = vec![95.0, f64::NAN];
        v.run_angle = vec![80.0, 200.0];
        let polar = vpp_to_polar(&v);
        polar.validate().unwrap();
        assert_eq!(polar.twa, vec![52.0, 90.0, 150.0]);
        assert_eq!(crate::cell_count(&polar), 5);
    }
}
