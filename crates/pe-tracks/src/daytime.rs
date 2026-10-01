//! The time of day a sample was sailed at, where it was sailed (spec.md
//! 10.2): local mean solar time, UTC shifted by longitude at 15° an hour.
//!
//! A clock rule, not the sun's: no time-zone table and no ephemeris, so the
//! band depends only on the instant and the longitude, the same on every
//! machine. It is a display grouping; it never enters the blend.

/// Seconds in a day.
const DAY_S: f64 = 86_400.0;
/// Seconds of local solar time per degree of longitude (24 h over 360°).
const SECONDS_PER_DEGREE: f64 = 240.0;

/// A band of the local solar day (settled with the user 2026-10-01).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum DayBand {
    /// 21:00 up to 05:00.
    Night,
    /// 05:00 up to 12:00.
    Morning,
    /// 12:00 up to 17:00.
    Afternoon,
    /// 17:00 up to 21:00.
    Evening,
}

impl DayBand {
    /// The two-bit code the dot packets carry (`pe-app`'s `polar3d` and
    /// `polar_plot`).
    pub fn code(self) -> u32 {
        match self {
            Self::Night => 0,
            Self::Morning => 1,
            Self::Afternoon => 2,
            Self::Evening => 3,
        }
    }
}

/// Local mean solar time at `lon` (degrees, east positive) at the UTC
/// instant `t` (epoch seconds), in hours in [0, 24).
pub fn local_solar_hour(t: i64, lon: f64) -> f64 {
    // Whole seconds of the UTC day first, so a time far from the epoch
    // loses nothing to the float; `rem_euclid` keeps both a time before
    // 1970 and a western longitude counting forward from midnight.
    let utc = t.rem_euclid(DAY_S as i64) as f64;
    let hour = (utc + lon * SECONDS_PER_DEGREE).rem_euclid(DAY_S) / 3600.0;
    // A sum a hair under a whole day can round up to 24.0 in the division.
    if hour >= 24.0 { 0.0 } else { hour }
}

/// The band of the local solar day `t` falls in at `lon`.
pub fn day_band(t: i64, lon: f64) -> DayBand {
    let hour = local_solar_hour(t, lon);
    if (5.0..12.0).contains(&hour) {
        DayBand::Morning
    } else if (12.0..17.0).contains(&hour) {
        DayBand::Afternoon
    } else if (17.0..21.0).contains(&hour) {
        DayBand::Evening
    } else {
        DayBand::Night
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HOUR: i64 = 3600;

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    /// 1,700,000,000 s is 2023-11-14 22:13:20 UTC (19,675 days and 80,000 s):
    /// 22.2222… h at Greenwich, five hours earlier at 75°W, ten hours later
    /// (past midnight) at 150°E.
    #[test]
    fn the_hour_is_utc_shifted_by_fifteen_degrees_an_hour() {
        let t = 1_700_000_000;
        let utc = 22.0 + 13.0 / 60.0 + 20.0 / 3600.0;
        assert!(close(local_solar_hour(t, 0.0), utc));
        assert!(close(local_solar_hour(t, -75.0), utc - 5.0));
        assert!(close(local_solar_hour(t, 150.0), utc + 10.0 - 24.0));
        assert!(close(local_solar_hour(t, -150.0), utc - 10.0));
    }

    #[test]
    fn the_same_instant_is_a_different_band_around_the_world() {
        let t = 1_700_000_000;
        assert_eq!(day_band(t, 0.0), DayBand::Night); // 22:13
        assert_eq!(day_band(t, -75.0), DayBand::Evening); // 17:13
        assert_eq!(day_band(t, 150.0), DayBand::Morning); // 08:13
        assert_eq!(day_band(t, -150.0), DayBand::Afternoon); // 12:13
    }

    /// Each band starts on its hour and ends just before the next one's.
    #[test]
    fn bands_change_on_the_hour() {
        for (hour, before, from) in [
            (5, DayBand::Night, DayBand::Morning),
            (12, DayBand::Morning, DayBand::Afternoon),
            (17, DayBand::Afternoon, DayBand::Evening),
            (21, DayBand::Evening, DayBand::Night),
        ] {
            assert_eq!(day_band(hour * HOUR - 1, 0.0), before, "{hour}:00 less 1 s");
            assert_eq!(day_band(hour * HOUR, 0.0), from, "{hour}:00");
        }
        assert_eq!(day_band(0, 0.0), DayBand::Night, "midnight");
    }

    /// Noon UTC is midnight on the antimeridian, from either side, and the
    /// hour stays in [0, 24) there.
    #[test]
    fn the_antimeridian_is_one_place() {
        let noon = 12 * HOUR;
        assert!(close(local_solar_hour(noon, 180.0), 0.0));
        assert!(close(local_solar_hour(noon, -180.0), 0.0));
        assert!(close(local_solar_hour(noon, 179.5), 24.0 - 0.5 / 15.0));
        assert!(close(local_solar_hour(noon, -179.5), 0.5 / 15.0));
        for lon in [180.0, -180.0, 179.5, -179.5] {
            let hour = local_solar_hour(noon, lon);
            assert!((0.0..24.0).contains(&hour), "{lon}: {hour}");
            assert_eq!(day_band(noon, lon), DayBand::Night);
        }
    }

    /// An instant before 1970 counts back from midnight, not forward.
    #[test]
    fn times_before_the_epoch_keep_their_hour() {
        assert!(close(local_solar_hour(-HOUR, 0.0), 23.0));
        assert_eq!(day_band(-HOUR, 0.0), DayBand::Night);
        assert!(close(local_solar_hour(-86_400 * 3 + 6 * HOUR, 0.0), 6.0));
    }

    /// The codes the dot packets and the frontend's legend share.
    #[test]
    fn codes_are_the_documented_ones() {
        assert_eq!(
            [
                DayBand::Night,
                DayBand::Morning,
                DayBand::Afternoon,
                DayBand::Evening
            ]
            .map(DayBand::code),
            [0, 1, 2, 3]
        );
    }
}
