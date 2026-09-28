//! Heading and speed for every fix (spec.md 7.4).
//!
//! A track is first reduced to fixes in time order with one fix per
//! timestamp ([`normalise`]). Then each fix gets a heading and a speed: the
//! track's own where it gave one, and otherwise one derived from its
//! neighbours by a central difference — the initial great-circle bearing
//! from the previous fix to the next, and the distance through this fix over
//! the time between them. A neighbour further away in time than the
//! track's maximum gap is not used; a fix with one usable neighbour (the
//! first and last fixes, or either side of a gap) uses that one, and a fix
//! with none gets no derived values.

use pe_core::track::{DerivationSettings, Fix, Motion, PreferValues, ValueOrigin};

use crate::geo::{KNOT_M_S, distance_m, initial_bearing_deg, wrap_360};

/// What [`normalise`] had to do, for the import summary (spec.md 7.4).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct NormaliseReport {
    /// Fixes that came earlier in time than the fix before them in the file.
    pub out_of_order: usize,
    /// Fixes merged into another with the same timestamp.
    pub duplicates: usize,
}

/// Sorts fixes by time and merges those sharing a timestamp.
///
/// The sort is stable, so of several fixes at one time the one that came
/// first in the file keeps its position; a heading or speed only a later
/// duplicate carries is kept too, so merging never throws a given value
/// away.
pub fn normalise(mut fixes: Vec<Fix>) -> (Vec<Fix>, NormaliseReport) {
    let out_of_order = fixes
        .windows(2)
        .filter(|pair| pair[1].t < pair[0].t)
        .count();
    fixes.sort_by_key(|fix| fix.t);
    let mut merged: Vec<Fix> = Vec::with_capacity(fixes.len());
    let mut duplicates = 0;
    for fix in fixes {
        match merged.last_mut() {
            Some(kept) if kept.t == fix.t => {
                duplicates += 1;
                kept.cog = kept.cog.or(fix.cog);
                kept.sog = kept.sog.or(fix.sog);
            }
            _ => merged.push(fix),
        }
    }
    (
        merged,
        NormaliseReport {
            out_of_order,
            duplicates,
        },
    )
}

/// The heading and speed a fix's neighbours give it, if any.
fn derived_at(fixes: &[Fix], i: usize, max_gap_s: i64) -> (Option<f64>, Option<f64>) {
    let this = &fixes[i];
    let usable = |other: &Fix| (this.t - other.t).abs() <= max_gap_s;
    let prev = i.checked_sub(1).map(|k| &fixes[k]).filter(|f| usable(f));
    let next = fixes.get(i + 1).filter(|f| usable(f));
    let leg = |a: &Fix, b: &Fix| distance_m(a.lat, a.lon, b.lat, b.lon);
    let (from, to, metres) = match (prev, next) {
        (Some(p), Some(n)) => (p, n, leg(p, this) + leg(this, n)),
        (Some(p), None) => (p, this, leg(p, this)),
        (None, Some(n)) => (this, n, leg(this, n)),
        (None, None) => return (None, None),
    };
    let seconds = (to.t - from.t) as f64;
    let speed = (seconds > 0.0).then(|| metres / seconds / KNOT_M_S);
    let heading = initial_bearing_deg(from.lat, from.lon, to.lat, to.lon);
    (heading, speed)
}

/// Picks between a given and a derived value, recording which was used.
fn choose(
    given: Option<f64>,
    derived: Option<f64>,
    prefer: PreferValues,
) -> (Option<f64>, Option<ValueOrigin>) {
    let given = given.map(|v| (v, ValueOrigin::Given));
    let derived = derived.map(|v| (v, ValueOrigin::Derived));
    let pick = match prefer {
        PreferValues::Given => given.or(derived),
        PreferValues::Derived => derived.or(given),
    };
    (pick.map(|(v, _)| v), pick.map(|(_, o)| o))
}

/// Every fix's motion. `fixes` must be as [`normalise`] leaves them.
pub fn derive(fixes: &[Fix], settings: &DerivationSettings) -> Vec<Motion> {
    (0..fixes.len())
        .map(|i| {
            let fix = &fixes[i];
            let (heading, speed) = derived_at(fixes, i, settings.max_gap_s);
            let given_heading = fix.cog.filter(|v| v.is_finite()).map(wrap_360);
            let given_speed = fix.sog.filter(|v| v.is_finite() && *v >= 0.0);
            let (heading, heading_origin) = choose(given_heading, heading, settings.prefer);
            let (speed, speed_origin) = choose(given_speed, speed, settings.prefer);
            Motion {
                heading,
                heading_origin,
                speed,
                speed_origin,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fix(t: i64, lat: f64, lon: f64) -> Fix {
        Fix {
            t,
            lat,
            lon,
            cog: None,
            sog: None,
        }
    }

    fn close(a: Option<f64>, b: f64, tol: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() <= tol)
    }

    fn default() -> DerivationSettings {
        DerivationSettings::default()
    }

    /// 0.01° of arc on the 6,371,229 m sphere is 1,111.98923 m. Two of them
    /// in 1,200 s is 1.8533154 m/s, which is 3.6025569 kn (1 kn = 1852/3600
    /// m/s).
    const HUNDREDTH_M: f64 = 1_111.989_234_5;
    const TWO_HUNDREDTHS_IN_20_MIN_KN: f64 = 3.602_556_9;

    #[test]
    fn a_boat_sailing_east_on_the_equator() {
        assert!((HUNDREDTH_M - 6_371_229.0 * std::f64::consts::PI / 180.0 * 0.01).abs() < 1e-6);
        let fixes = [fix(0, 0.0, 0.0), fix(600, 0.0, 0.01), fix(1200, 0.0, 0.02)];
        let motion = derive(&fixes, &default());
        for m in &motion {
            assert!(close(m.heading, 90.0, 1e-9), "{m:?}");
            assert!(close(m.speed, TWO_HUNDREDTHS_IN_20_MIN_KN, 1e-6), "{m:?}");
            assert_eq!(m.heading_origin, Some(ValueOrigin::Derived));
            assert_eq!(m.speed_origin, Some(ValueOrigin::Derived));
        }
    }

    /// Central difference: the middle fix's heading is the bearing from the
    /// previous fix to the next, whatever the middle fix does; its speed is
    /// the distance through it. (0,0) → (0.01,0.01) → (0,0.02): heading
    /// 90°, and 2 × 1,572.5 m in 1,200 s = 5.0947848 kn. The first fix uses
    /// its one neighbour: bearing 44.9999° (tan⁻¹ of sin 0.01°·cos 0.01° /
    /// sin 0.01°), same speed.
    #[test]
    fn a_central_difference_skips_over_the_middle_fix() {
        let fixes = [fix(0, 0.0, 0.0), fix(600, 0.01, 0.01), fix(1200, 0.0, 0.02)];
        let motion = derive(&fixes, &default());
        assert!(close(motion[1].heading, 90.0, 1e-9));
        assert!(close(motion[1].speed, 5.094_784_8, 1e-6));
        let r = 0.01f64.to_radians();
        let first = (r.sin() * r.cos()).atan2(r.sin()).to_degrees();
        assert!(close(motion[0].heading, first, 1e-9));
        assert!(close(motion[0].heading, 44.999_9, 1e-4));
        assert!(close(motion[0].speed, 5.094_784_8, 1e-6));
        // The last fix: from (0.01, 0.01) down to (0, 0.02), south-east:
        // 134.9999996°.
        assert!(close(motion[2].heading, 135.0, 1e-5));
    }

    #[test]
    fn a_boat_crossing_the_antimeridian_both_ways() {
        let east = [
            fix(0, 0.0, 179.99),
            fix(600, 0.0, -180.0),
            fix(1200, 0.0, -179.99),
        ];
        for m in derive(&east, &default()) {
            assert!(close(m.heading, 90.0, 1e-9), "{m:?}");
            assert!(close(m.speed, TWO_HUNDREDTHS_IN_20_MIN_KN, 1e-6), "{m:?}");
        }
        let west = [
            fix(0, 0.0, -179.99),
            fix(600, 0.0, 180.0),
            fix(1200, 0.0, 179.99),
        ];
        for m in derive(&west, &default()) {
            assert!(close(m.heading, 270.0, 1e-9), "{m:?}");
            assert!(close(m.speed, TWO_HUNDREDTHS_IN_20_MIN_KN, 1e-6), "{m:?}");
        }
    }

    /// Northward at 50°N: 0.1° of latitude in an hour is 11,119.892 m / 3,600
    /// s = 6.0042615 kn, heading 0°.
    #[test]
    fn a_boat_sailing_north() {
        let fixes = [fix(0, 50.0, -1.0), fix(3600, 50.1, -1.0)];
        for m in derive(&fixes, &default()) {
            assert!(close(m.heading, 0.0, 1e-9), "{m:?}");
            assert!(close(m.speed, 6.004_261_5, 1e-6), "{m:?}");
        }
    }

    /// Over the North Pole: from 89.99°N 0° to 89.99°N 180° is 0.02° of arc
    /// (2,223.98 m) in 600 s = 7.2051138 kn, and the way there is due north.
    #[test]
    fn a_boat_crossing_the_pole() {
        let fixes = [fix(0, 89.99, 0.0), fix(600, 89.99, 180.0)];
        let motion = derive(&fixes, &default());
        let north = |h: Option<f64>| h.is_some_and(|h| crate::geo::angle_between(h, 0.0) < 1e-6);
        assert!(north(motion[0].heading), "{:?}", motion[0]);
        assert!(close(motion[0].speed, 7.205_113_8, 1e-6));
        // Seen from the far side, the pole is north too.
        assert!(north(motion[1].heading), "{:?}", motion[1]);
    }

    #[test]
    fn a_stationary_boat_has_no_heading_and_no_speed() {
        let fixes = [
            fix(0, 50.0, -1.0),
            fix(600, 50.0, -1.0),
            fix(1200, 50.0, -1.0),
        ];
        for m in derive(&fixes, &default()) {
            assert_eq!(m.heading, None);
            assert_eq!(m.heading_origin, None);
            assert_eq!(m.speed, Some(0.0));
            assert_eq!(m.speed_origin, Some(ValueOrigin::Derived));
        }
    }

    #[test]
    fn a_single_fix_track_derives_nothing_but_keeps_what_it_gave() {
        let alone = [fix(0, 50.0, -1.0)];
        assert_eq!(derive(&alone, &default()), vec![Motion::default()]);
        let given = [Fix {
            cog: Some(-90.0),
            sog: Some(6.5),
            ..fix(0, 50.0, -1.0)
        }];
        let motion = derive(&given, &default());
        assert_eq!(motion[0].heading, Some(270.0));
        assert_eq!(motion[0].heading_origin, Some(ValueOrigin::Given));
        assert_eq!(motion[0].speed, Some(6.5));
        assert_eq!(motion[0].speed_origin, Some(ValueOrigin::Given));
        assert!(derive(&[], &default()).is_empty());
    }

    /// A neighbour more than the maximum gap away is not used: the fix
    /// before a four-hour gap uses the one before it, and a fix with gaps on
    /// both sides gets nothing.
    #[test]
    fn neighbours_beyond_the_maximum_gap_are_not_used() {
        let four_hours = 4 * 3600;
        let fixes = [
            fix(0, 0.0, 0.0),
            fix(600, 0.0, 0.01),
            fix(600 + four_hours, 0.0, 1.0),
            fix(600 + 2 * four_hours, 0.0, 2.0),
        ];
        let motion = derive(&fixes, &default());
        // One-sided from the fix before: 0.01° in 600 s.
        assert!(close(motion[1].heading, 90.0, 1e-9));
        assert!(close(motion[1].speed, TWO_HUNDREDTHS_IN_20_MIN_KN, 1e-6));
        assert_eq!(motion[2], Motion::default());
        assert_eq!(motion[3], Motion::default());
        // A longer maximum gap reaches across.
        let wide = DerivationSettings {
            max_gap_s: 5 * 3600,
            ..default()
        };
        assert!(derive(&fixes, &wide)[2].speed.is_some());
    }

    #[test]
    fn given_values_win_unless_derived_ones_are_preferred() {
        let mut fixes = vec![fix(0, 0.0, 0.0), fix(600, 0.0, 0.01), fix(1200, 0.0, 0.02)];
        fixes[1].cog = Some(95.0);
        fixes[1].sog = Some(4.0);
        let given = derive(&fixes, &default());
        assert_eq!(given[1].heading, Some(95.0));
        assert_eq!(given[1].heading_origin, Some(ValueOrigin::Given));
        assert_eq!(given[1].speed, Some(4.0));
        assert_eq!(given[0].heading_origin, Some(ValueOrigin::Derived));
        let prefer_derived = DerivationSettings {
            prefer: PreferValues::Derived,
            ..default()
        };
        let derived = derive(&fixes, &prefer_derived);
        assert!(close(derived[1].heading, 90.0, 1e-9));
        assert_eq!(derived[1].speed_origin, Some(ValueOrigin::Derived));
        // With nothing to derive from, a preferred-derived fix falls back to
        // what it was given.
        let alone = [fixes[1].clone()];
        assert_eq!(derive(&alone, &prefer_derived)[0].heading, Some(95.0));
    }

    #[test]
    fn duplicates_are_merged_and_disorder_sorted_and_both_counted() {
        let mut late = fix(600, 0.0, 0.01);
        late.sog = Some(3.0);
        let fixes = vec![
            fix(1200, 0.0, 0.02),
            fix(0, 0.0, 0.0),
            fix(600, 0.0, 0.011),
            late,
            fix(600, 0.0, 0.012),
        ];
        let (sorted, report) = normalise(fixes);
        assert_eq!(
            sorted.iter().map(|f| f.t).collect::<Vec<_>>(),
            [0, 600, 1200]
        );
        // The first of the duplicates keeps its position; the later one's
        // speed is kept rather than lost.
        assert_eq!(sorted[1].lon, 0.011);
        assert_eq!(sorted[1].sog, Some(3.0));
        assert_eq!(
            report,
            NormaliseReport {
                out_of_order: 1,
                duplicates: 2
            }
        );
    }
}
