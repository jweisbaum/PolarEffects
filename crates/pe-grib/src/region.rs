//! The area and times of a reanalysis export (spec.md 7.8).
//!
//! **Area.** The track's bounding box plus a margin, snapped outward to the
//! archive's native 0.25° grid, so every node of the file is a node of
//! ERA5 and the wind is written as the archive has it, not interpolated.
//! Longitudes are handled on the circle: the box is the shortest arc that
//! holds every position, so a race across the antimeridian (Sydney–Hobart
//! is not one; the Transpac and a Pacific crossing are) gets a box of a few
//! degrees across 180°, not one spanning the globe the other way.
//!
//! **Times.** Every step of the interval from the hour at or before the
//! first fix to the hour at or after the last, aligned on UTC midnight.

use crate::writer::GridSpec;

/// The native ERA5 spacing, micro-degrees.
pub const NATIVE_STEP: u32 = 250_000;

/// The margin around the track, degrees (spec.md 7.8).
pub const MARGIN_DEG: f64 = 2.0;

const TURN: i64 = 360_000_000;

/// The grid covering `positions` (latitude, longitude in degrees, any
/// longitude convention) plus `margin` degrees on every side, on a grid of
/// `step` micro-degrees aligned on 0°. `None` without a finite position.
///
/// Latitudes are clamped to the poles. When the longitudes plus the margin
/// go all the way round, the grid is global in longitude from 0°E.
pub fn region(positions: &[(f64, f64)], margin: f64, step: u32) -> Option<GridSpec> {
    let finite: Vec<(f64, f64)> = positions
        .iter()
        .copied()
        .filter(|(lat, lon)| lat.is_finite() && lon.is_finite())
        .collect();
    if finite.is_empty() || step == 0 || !margin.is_finite() || margin < 0.0 {
        return None;
    }
    let step_deg = f64::from(step) / 1e6;
    let per_turn = TURN / i64::from(step);

    // Latitude: grid rows are k * step; the north edge is rounded up, the
    // south edge down, both inside the poles.
    let (mut south, mut north) = (f64::INFINITY, f64::NEG_INFINITY);
    for (lat, _) in &finite {
        south = south.min(*lat);
        north = north.max(*lat);
    }
    let half = per_turn / 4;
    let n_idx = (((north + margin) / step_deg).ceil() as i64).clamp(-half, half);
    let s_idx = (((south - margin) / step_deg).floor() as i64).clamp(-half, half);
    let nj = n_idx - s_idx + 1;

    // Longitude: the shortest arc holding every position is the circle
    // less its largest empty gap.
    let mut lons: Vec<f64> = finite
        .iter()
        .map(|(_, lon)| lon.rem_euclid(360.0))
        .collect();
    lons.sort_by(f64::total_cmp);
    let mut start = lons[0];
    let mut gap = lons[0] + 360.0 - lons[lons.len() - 1];
    for pair in lons.windows(2) {
        let g = pair[1] - pair[0];
        if g > gap {
            gap = g;
            start = pair[1];
        }
    }
    let extent = 360.0 - gap;
    let (lo1_idx, ni) = if extent + 2.0 * margin >= 360.0 - step_deg {
        (0, per_turn)
    } else {
        let west = ((start - margin) / step_deg).floor() as i64;
        let east = ((start + extent + margin) / step_deg).ceil() as i64;
        let ni = (east - west + 1).min(per_turn);
        (west.rem_euclid(per_turn), ni)
    };

    Some(GridSpec {
        ni: u32::try_from(ni).ok()?,
        nj: u32::try_from(nj).ok()?,
        la1: i32::try_from(n_idx * i64::from(step)).ok()?,
        lo1: u32::try_from(lo1_idx * i64::from(step)).ok()?,
        step,
    })
}

/// The times written: every `every` seconds (aligned on UTC midnight) from
/// the step at or before `first` to the step at or after `last`.
pub fn times(first: i64, last: i64, every: i64) -> Vec<i64> {
    if every <= 0 || last < first {
        return Vec::new();
    }
    let start = first - first.rem_euclid(every);
    let end = if last.rem_euclid(every) == 0 {
        last
    } else {
        last - last.rem_euclid(every) + every
    };
    (0..=(end - start) / every)
        .map(|k| start + k * every)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn deg(micro: i64) -> f64 {
        micro as f64 / 1e6
    }

    /// Hand-computed: a Channel track 49.9–50.8N, 1.3–9.5W plus 2° is
    /// 47.9–52.8N → rows 53.0 (52.8 rounded up) down to 47.75 (47.9
    /// rounded down), 22 rows; and 11.5W–0.7E → columns 348.5E to 0.75E,
    /// across the prime meridian: 50 columns (11.5 + 0.75 = 12.25° / 0.25
    /// + 1).
    #[test]
    fn a_channel_track_crosses_the_prime_meridian() {
        let g = region(
            &[(50.8, -1.3), (49.9, -9.5), (50.1, -5.0)],
            MARGIN_DEG,
            NATIVE_STEP,
        )
        .expect("a region");
        assert_eq!(deg(i64::from(g.la1)), 53.0);
        assert_eq!(deg(i64::from(g.la2())), 47.75);
        assert_eq!(g.nj, 22);
        assert_eq!(deg(i64::from(g.lo1)), 348.5);
        assert_eq!(g.ni, 50);
        assert_eq!(deg(i64::from(g.lo2())), 0.75);
        // The first and last points of a row, as the app sees them.
        let row: Vec<(f64, f64)> = g.points().take(g.ni as usize).collect();
        assert_eq!(row[0], (-11.5, 53.0));
        assert_eq!(row[46], (0.0, 53.0));
        assert_eq!(row[49], (0.75, 53.0));
    }

    /// A Pacific crossing from 170E to 170W is 20° wide across the
    /// antimeridian, not 340° the other way: 168E to 192E (168W), 97
    /// columns, and the row runs through 180 into negative longitudes.
    #[test]
    fn an_antimeridian_track_takes_the_short_way_round() {
        let g = region(
            &[(20.0, 170.0), (21.0, -170.0), (20.5, 179.9)],
            2.0,
            NATIVE_STEP,
        )
        .expect("a region");
        assert_eq!(deg(i64::from(g.lo1)), 168.0);
        assert_eq!(g.ni, 97);
        assert_eq!(deg(i64::from(g.lo2())), 192.0);
        let row: Vec<(f64, f64)> = g.points().take(g.ni as usize).collect();
        assert_eq!(row[0].0, 168.0);
        assert_eq!(row[48].0, -180.0);
        assert_eq!(row[96].0, -168.0);
        // Both longitude conventions give the same box.
        let same = region(
            &[(20.0, 170.0), (21.0, 190.0), (20.5, 179.9)],
            2.0,
            NATIVE_STEP,
        );
        assert_eq!(same, Some(g));
    }

    /// A circumnavigation is global in longitude from 0°E, and a track
    /// near a pole stops at the pole. A track that leaves a gap wider than
    /// the margins is not: every 10° round the world leaves 6° out.
    #[test]
    fn a_round_the_world_track_is_global_and_poles_clamp() {
        // Every 2°: the largest gap is 2°, less than the two margins.
        let lons: Vec<(f64, f64)> = (0..180).map(|k| (-40.0, -180.0 + 2.0 * k as f64)).collect();
        let g = region(&lons, 2.0, NATIVE_STEP).expect("a region");
        assert_eq!((g.lo1, g.ni), (0, 1440));
        assert_eq!(deg(i64::from(g.lo2())), 359.75);
        let sparse: Vec<(f64, f64)> = (0..36).map(|k| (-40.0, -180.0 + 10.0 * k as f64)).collect();
        let g = region(&sparse, 2.0, NATIVE_STEP).expect("a region");
        // Every gap is 10°; the first (across 0°) is taken: 0E to 350E
        // + 2° each side, 358E round to 352E.
        assert_eq!((deg(i64::from(g.lo1)), g.ni), (358.0, 1417));
        let polar = region(&[(89.0, 10.0), (-89.5, 12.0)], 2.0, NATIVE_STEP).expect("a region");
        assert_eq!(deg(i64::from(polar.la1)), 90.0);
        assert_eq!(deg(i64::from(polar.la2())), -90.0);
        assert_eq!(polar.nj, 721);
    }

    /// A boat that never moved still gets the margin all round: 4° plus a
    /// node, both ways.
    #[test]
    fn a_stationary_boat_gets_a_box_of_the_margin() {
        let g = region(&[(50.0, -5.0); 3], 2.0, NATIVE_STEP).expect("a region");
        assert_eq!((g.ni, g.nj), (17, 17));
        assert_eq!(deg(i64::from(g.la1)), 52.0);
        assert_eq!(deg(i64::from(g.lo1)), 353.0);
        assert_eq!(region(&[], 2.0, NATIVE_STEP), None);
        assert_eq!(region(&[(f64::NAN, 0.0)], 2.0, NATIVE_STEP), None);
    }

    #[test]
    fn times_cover_the_track_on_whole_steps() {
        // 00:10 to 02:00: 00, 01, 02.
        assert_eq!(times(600, 7200, 3600), vec![0, 3600, 7200]);
        // 00:10 to 02:10: 00 to 03.
        assert_eq!(times(600, 7800, 3600).len(), 4);
        // Every 3 hours from 04:00 to 07:30: 03, 06, 09.
        assert_eq!(
            times(4 * 3600, 7 * 3600 + 1800, 3 * 3600),
            vec![3 * 3600, 6 * 3600, 9 * 3600]
        );
        // One fix on the hour: that hour alone.
        assert_eq!(times(3600, 3600, 3600), vec![3600]);
        assert!(times(10, 0, 3600).is_empty());
        // Before 1970.
        assert_eq!(times(-5400, -1800, 3600), vec![-7200, -3600, 0]);
    }
}
