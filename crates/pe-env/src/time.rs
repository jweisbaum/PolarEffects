//! Time axes. Every archive keeps time as a number of units since an epoch
//! written in a CF `units` attribute; everything else in PolarEffects uses
//! UTC epoch seconds (CLAUDE.md conventions), so axes are converted once, on
//! open.

use crate::error::{EnvError, Result};

/// Days from 1970-01-01 to a proleptic Gregorian date (Howard Hinnant's
/// `days_from_civil`).
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = if month <= 2 { year - 1 } else { year };
    let era = y.div_euclid(400);
    let yoe = y - era * 400;
    let m = i64::from(month);
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// The date of a day number, inverse of [`days_from_civil`].
fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let month = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let year = yoe + era * 400 + i64::from(month <= 2);
    (year, month, day)
}

/// UTC epoch seconds as `YYYY-MM-DDTHH:MM:SSZ`, for messages.
pub fn to_iso(t: i64) -> String {
    let (days, secs) = (t.div_euclid(86_400), t.rem_euclid(86_400));
    let (y, m, d) = civil_from_days(days);
    format!(
        "{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}Z",
        secs / 3600,
        secs / 60 % 60,
        secs % 60
    )
}

/// Parses `YYYY-MM-DD[( |T)HH[:MM[:SS]]][Z|+00:00]` as UTC epoch seconds.
pub fn parse_utc(text: &str) -> Option<i64> {
    let text = text.trim();
    let text = text
        .strip_suffix("+00:00")
        .or_else(|| text.strip_suffix('Z'))
        .unwrap_or(text)
        .trim();
    let (date, clock) = match text.split_once(['T', ' ']) {
        Some((date, clock)) => (date, clock.trim()),
        None => (text, ""),
    };
    let mut parts = date.split('-');
    let year: i64 = parts.next()?.parse().ok()?;
    let month: u32 = parts.next()?.parse().ok()?;
    let day: u32 = parts.next()?.parse().ok()?;
    if parts.next().is_some() || !(1..=12).contains(&month) || !(1..=31).contains(&day) {
        return None;
    }
    let mut hms = [0i64; 3];
    if !clock.is_empty() {
        for (slot, part) in hms.iter_mut().zip(clock.split(':')) {
            // Fractional seconds would change nothing on an hourly axis but
            // must not make the epoch unreadable.
            *slot = part.split('.').next()?.parse().ok()?;
        }
    }
    let [h, mi, s] = hms;
    if !(0..24).contains(&h) || !(0..60).contains(&mi) || !(0..61).contains(&s) {
        return None;
    }
    Some(days_from_civil(year, month, day) * 86_400 + h * 3600 + mi * 60 + s)
}

/// A CF time `units` attribute: how many seconds one unit is, and the epoch.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeUnits {
    /// Seconds per unit.
    pub seconds: i64,
    /// The epoch, UTC epoch seconds.
    pub epoch: i64,
}

impl TimeUnits {
    /// Parses `"<unit> since <date>"`.
    ///
    /// # Errors
    /// [`EnvError::Layout`] for a unit other than days, hours, minutes or
    /// seconds, or an epoch that does not parse. Guessing would silently
    /// shift every index.
    pub fn parse(units: &str) -> Result<Self> {
        let (unit, since) = units.split_once(" since ").ok_or_else(|| {
            EnvError::Layout(format!(
                "the time axis units {units:?} are not \"<unit> since <date>\""
            ))
        })?;
        let seconds = match unit.trim() {
            "days" => 86_400,
            "hours" => 3600,
            "minutes" => 60,
            "seconds" => 1,
            other => {
                return Err(EnvError::Layout(format!(
                    "the time axis is measured in {other:?}, which this reader does not handle"
                )));
            }
        };
        let epoch = parse_utc(since)
            .ok_or_else(|| EnvError::Layout(format!("could not parse the time epoch {since:?}")))?;
        Ok(Self { seconds, epoch })
    }

    /// Converts one axis value, which must be a whole number of units.
    ///
    /// # Errors
    /// [`EnvError::Layout`] if the value is not finite or not whole.
    pub fn to_epoch(self, value: f64, index: usize) -> Result<i64> {
        if !value.is_finite() || value.fract() != 0.0 {
            return Err(EnvError::Layout(format!(
                "time axis entry {index} is {value}, not a whole number of units"
            )));
        }
        Ok(self.epoch + value as i64 * self.seconds)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_dates_have_known_day_numbers() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
        assert_eq!(days_from_civil(2000, 3, 1), 11_017);
        assert_eq!(days_from_civil(1959, 1, 1), -4018);
        assert_eq!(days_from_civil(1900, 1, 1), -25_567);
        for days in [-40_000, -25_567, -1, 0, 1, 11_017, 20_000, 60_000] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days);
        }
    }

    /// The three archives' own units strings.
    #[test]
    fn the_archive_epochs_parse() {
        let wb2 = TimeUnits::parse("hours since 1959-01-01 00:00:00").expect("wb2");
        assert_eq!(
            wb2,
            TimeUnits {
                seconds: 3600,
                epoch: -347_155_200
            }
        );
        let arco = TimeUnits::parse("hours since 1900-01-01").expect("arco");
        assert_eq!(arco.epoch, -2_208_988_800);
        let cmems = TimeUnits::parse("hours since 1950-01-01").expect("cmems");
        // 620928 hours after 1950 is the first hour of the merged dataset.
        assert_eq!(
            to_iso(cmems.to_epoch(620_928.0, 0).expect("whole")),
            "2020-11-01T00:00:00Z"
        );
        let gc = TimeUnits::parse("hours since 1950-01-01T00:00:00+00:00").expect("globcurrent");
        assert_eq!(gc.epoch, cmems.epoch);
    }

    /// WeatherBench2's index for the M9 acceptance hour, worked by hand:
    /// 2020-07-27T12Z is 539724 hours after 1959-01-01.
    #[test]
    fn a_hand_computed_index_maps_to_its_hour() {
        let wb2 = TimeUnits::parse("hours since 1959-01-01 00:00:00").expect("wb2");
        assert_eq!(
            to_iso(wb2.to_epoch(539_724.0, 0).expect("whole")),
            "2020-07-27T12:00:00Z"
        );
    }

    #[test]
    fn unsupported_units_and_fractions_are_refused() {
        assert!(TimeUnits::parse("fortnights since 1970-01-01").is_err());
        assert!(TimeUnits::parse("hours after 1970-01-01").is_err());
        assert!(TimeUnits::parse("hours since yesterday").is_err());
        let h = TimeUnits::parse("hours since 1970-01-01").expect("parses");
        assert!(h.to_epoch(1.5, 3).is_err());
        assert!(h.to_epoch(f64::NAN, 3).is_err());
    }
}
