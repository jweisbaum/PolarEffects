//! Reading timestamps from track files (spec.md 7.3): ISO 8601, epoch
//! seconds or milliseconds, or a format string the user gives. Every result
//! is UTC epoch seconds; a time with no zone is taken as UTC, since track
//! files are UTC by convention and a local zone would be a guess.

/// How a time column is written.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum TimeFormat {
    /// A number is epoch seconds (or milliseconds, from 10^11 up), anything
    /// else ISO 8601.
    #[default]
    Auto,
    /// ISO 8601: `2025-07-26T12:00:00Z`, `2025-07-26 12:00`, with an optional
    /// fraction and zone offset. `/` is accepted between date parts.
    Iso8601,
    /// Seconds since 1970-01-01T00:00:00Z.
    EpochSeconds,
    /// Milliseconds since 1970-01-01T00:00:00Z.
    EpochMillis,
    /// A `strftime`-style pattern: `%Y %y %m %d %H %M %S %f %b %z %%`, any
    /// other character matched as itself.
    Custom(String),
}

/// Why a time did not read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum TimeError {
    /// Nothing there.
    #[error("the time is empty")]
    Empty,
    /// Not in the expected format.
    #[error("{0:?} is not a time in the expected format")]
    Unreadable(String),
    /// A date that does not exist, such as 31 February.
    #[error("{0:?} is not a real date and time")]
    OutOfRange(String),
    /// A format string with an unknown `%` code.
    #[error("the format {0:?} has a code this reader does not know")]
    BadFormat(String),
}

/// Epoch milliseconds start here: 10^11 seconds is the year 5138, while
/// 10^11 ms is 1973, so every real track time is unambiguous.
const MILLIS_FROM: f64 = 1e11;

/// Days from 1970-01-01 to a proleptic Gregorian date (H. Hinnant's
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

fn days_in_month(year: i64, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 => 29,
        2 => 28,
        _ => 0,
    }
}

/// The parts of a time, as read.
#[derive(Debug, Default)]
struct Parts {
    year: Option<i64>,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
    /// Fraction of a second.
    fraction: f64,
    /// Offset east of UTC, seconds.
    offset: i64,
}

impl Parts {
    fn epoch(&self, text: &str) -> Result<i64, TimeError> {
        let out = || TimeError::OutOfRange(text.to_owned());
        let year = self.year.ok_or_else(out)?;
        if !(1..=12).contains(&self.month)
            || self.day < 1
            || self.day > days_in_month(year, self.month)
            || self.hour > 24
            // 24:00:00 is the end of the day; nothing later is.
            || (self.hour == 24 && (self.minute, self.second, self.fraction) != (0, 0, 0.0))
            || self.minute > 59
            || self.second > 60
        {
            return Err(out());
        }
        let seconds = days_from_civil(year, self.month, self.day) * 86_400
            + i64::from(self.hour) * 3600
            + i64::from(self.minute) * 60
            + i64::from(self.second)
            - self.offset;
        Ok(seconds + self.fraction.round() as i64)
    }
}

/// A cursor over the text being read.
struct Cursor<'a> {
    text: &'a str,
    at: usize,
}

impl<'a> Cursor<'a> {
    fn peek(&self) -> Option<char> {
        self.text[self.at..].chars().next()
    }

    fn eat(&mut self, wanted: char) -> bool {
        if self.peek() == Some(wanted) {
            self.at += wanted.len_utf8();
            true
        } else {
            false
        }
    }

    fn eat_any(&mut self, wanted: &[char]) -> bool {
        wanted.iter().any(|c| self.eat(*c))
    }

    /// Between `min` and `max` ASCII digits.
    fn digits(&mut self, min: usize, max: usize) -> Option<u64> {
        let rest = &self.text[self.at..];
        let len = rest
            .bytes()
            .take(max)
            .take_while(u8::is_ascii_digit)
            .count();
        if len < min {
            return None;
        }
        self.at += len;
        rest[..len].parse().ok()
    }

    /// A fraction of a second after the decimal point, as a value in [0, 1).
    fn fraction(&mut self) -> Option<f64> {
        let rest = &self.text[self.at..];
        let len = rest.bytes().take_while(u8::is_ascii_digit).count();
        if len == 0 {
            return None;
        }
        self.at += len;
        format!("0.{}", &rest[..len]).parse().ok()
    }

    /// `Z`, `±HH`, `±HHMM` or `±HH:MM`, as seconds east of UTC.
    fn zone(&mut self) -> Option<i64> {
        if self.eat_any(&['Z', 'z']) {
            return Some(0);
        }
        let start = self.at;
        let sign = if self.eat('+') {
            1
        } else if self.eat('-') {
            -1
        } else {
            return None;
        };
        let hours = self.digits(2, 2)? as i64;
        self.eat(':');
        let minutes = self.digits(2, 2).unwrap_or(0) as i64;
        // Real offsets run from -12:00 to +14:00; anything past ±14:00 or
        // with 60 minutes or more is a misread, not a zone.
        let offset = hours * 3600 + minutes * 60;
        let valid = minutes < 60 && offset <= 14 * 3600;
        if !valid {
            // Put the offset back, so the text does not read as finished.
            self.at = start;
        }
        valid.then_some(sign * offset)
    }

    fn done(&self) -> bool {
        self.at == self.text.len()
    }
}

fn parse_iso(text: &str) -> Result<i64, TimeError> {
    let bad = || TimeError::Unreadable(text.to_owned());
    let mut c = Cursor { text, at: 0 };
    let mut parts = Parts {
        year: Some(c.digits(4, 4).ok_or_else(bad)? as i64),
        ..Parts::default()
    };
    if !c.eat_any(&['-', '/']) {
        return Err(bad());
    }
    parts.month = c.digits(1, 2).ok_or_else(bad)? as u32;
    if !c.eat_any(&['-', '/']) {
        return Err(bad());
    }
    parts.day = c.digits(1, 2).ok_or_else(bad)? as u32;
    if c.eat_any(&['T', 't', ' ']) {
        parts.hour = c.digits(1, 2).ok_or_else(bad)? as u32;
        if !c.eat(':') {
            return Err(bad());
        }
        parts.minute = c.digits(2, 2).ok_or_else(bad)? as u32;
        if c.eat(':') {
            parts.second = c.digits(2, 2).ok_or_else(bad)? as u32;
            if c.eat_any(&['.', ',']) {
                parts.fraction = c.fraction().ok_or_else(bad)?;
            }
        }
        if let Some(offset) = c.zone() {
            parts.offset = offset;
        }
    }
    if !c.done() {
        return Err(bad());
    }
    parts.epoch(text)
}

fn parse_epoch(text: &str, scale: Option<f64>) -> Result<i64, TimeError> {
    let value: f64 = text
        .parse()
        .map_err(|_| TimeError::Unreadable(text.to_owned()))?;
    if !value.is_finite() {
        return Err(TimeError::Unreadable(text.to_owned()));
    }
    let scale = scale.unwrap_or(if value.abs() >= MILLIS_FROM {
        1e-3
    } else {
        1.0
    });
    let seconds = (value * scale).round();
    // Beyond a few thousand years is a misread column, not a race.
    if seconds.abs() > 1e11 {
        return Err(TimeError::OutOfRange(text.to_owned()));
    }
    Ok(seconds as i64)
}

const MONTHS: [&str; 12] = [
    "jan", "feb", "mar", "apr", "may", "jun", "jul", "aug", "sep", "oct", "nov", "dec",
];

fn parse_custom(text: &str, format: &str) -> Result<i64, TimeError> {
    let bad = || TimeError::Unreadable(text.to_owned());
    let mut c = Cursor { text, at: 0 };
    let mut parts = Parts {
        month: 1,
        day: 1,
        ..Parts::default()
    };
    let mut codes = format.chars();
    while let Some(code) = codes.next() {
        if code != '%' {
            if !c.eat(code) {
                return Err(bad());
            }
            continue;
        }
        let digits = |c: &mut Cursor<'_>, min, max| c.digits(min, max).ok_or_else(bad);
        match codes.next() {
            Some('Y') => parts.year = Some(digits(&mut c, 4, 4)? as i64),
            Some('y') => {
                let y = digits(&mut c, 2, 2)? as i64;
                parts.year = Some(if y < 70 { 2000 + y } else { 1900 + y });
            }
            Some('m') => parts.month = digits(&mut c, 1, 2)? as u32,
            Some('d') => parts.day = digits(&mut c, 1, 2)? as u32,
            Some('H') => parts.hour = digits(&mut c, 1, 2)? as u32,
            Some('M') => parts.minute = digits(&mut c, 1, 2)? as u32,
            Some('S') => parts.second = digits(&mut c, 1, 2)? as u32,
            Some('f') => parts.fraction = c.fraction().ok_or_else(bad)?,
            Some('z') => parts.offset = c.zone().ok_or_else(bad)?,
            Some('b') => {
                let rest = c.text[c.at..].to_ascii_lowercase();
                let month = MONTHS
                    .iter()
                    .position(|m| rest.starts_with(m))
                    .ok_or_else(bad)?;
                parts.month = month as u32 + 1;
                c.at += 3;
                // A full month name reads too.
                while c.peek().is_some_and(|ch| ch.is_ascii_alphabetic()) {
                    c.at += 1;
                }
            }
            Some('%') => {
                if !c.eat('%') {
                    return Err(bad());
                }
            }
            _ => return Err(TimeError::BadFormat(format.to_owned())),
        }
    }
    if !c.done() {
        return Err(bad());
    }
    parts.epoch(text)
}

/// Checks a user format string before any row is read with it.
pub fn check_format(format: &str) -> Result<(), TimeError> {
    let mut codes = format.chars();
    let mut year = false;
    while let Some(code) = codes.next() {
        if code == '%' {
            match codes.next() {
                Some('Y' | 'y') => year = true,
                Some('m' | 'd' | 'H' | 'M' | 'S' | 'f' | 'z' | 'b' | '%') => {}
                _ => return Err(TimeError::BadFormat(format.to_owned())),
            }
        }
    }
    if year {
        Ok(())
    } else {
        Err(TimeError::BadFormat(format.to_owned()))
    }
}

/// Reads one time, as UTC epoch seconds (fractions rounded to the nearest
/// second).
pub fn parse_time(text: &str, format: &TimeFormat) -> Result<i64, TimeError> {
    let text = text.trim();
    if text.is_empty() {
        return Err(TimeError::Empty);
    }
    match format {
        TimeFormat::Auto => {
            if text.parse::<f64>().is_ok() {
                parse_epoch(text, None)
            } else {
                parse_iso(text)
            }
        }
        TimeFormat::Iso8601 => parse_iso(text),
        TimeFormat::EpochSeconds => parse_epoch(text, Some(1.0)),
        TimeFormat::EpochMillis => parse_epoch(text, Some(1e-3)),
        TimeFormat::Custom(format) => parse_custom(text, format),
    }
}

/// The time format a sample of values most likely uses: epoch seconds or
/// milliseconds when every value is a number, else ISO 8601 when every one
/// reads as it, else `None`.
pub fn guess_format<'a>(values: impl IntoIterator<Item = &'a str>) -> Option<TimeFormat> {
    let values: Vec<&str> = values
        .into_iter()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .collect();
    if values.is_empty() {
        return None;
    }
    let numbers: Option<Vec<f64>> = values.iter().map(|v| v.parse().ok()).collect();
    if let Some(numbers) = numbers {
        let millis = numbers.iter().all(|v: &f64| v.abs() >= MILLIS_FROM);
        return Some(if millis {
            TimeFormat::EpochMillis
        } else {
            TimeFormat::EpochSeconds
        });
    }
    values
        .iter()
        .all(|v| parse_iso(v).is_ok())
        .then_some(TimeFormat::Iso8601)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 2025-07-26T12:00:00Z, from `date -u -d 2025-07-26T12:00:00Z +%s`.
    const NOON: i64 = 1_753_531_200;

    #[test]
    fn iso_times_with_and_without_zones() {
        let auto = TimeFormat::Auto;
        for text in [
            "2025-07-26T12:00:00Z",
            "2025-07-26T12:00:00",
            "2025-07-26 12:00",
            "2025/07/26 12:00:00",
            "2025-07-26T14:00:00+02:00",
            "2025-07-26T14:00:00+0200",
            "2025-07-26T09:30:00-02:30",
            "2025-07-26T12:00:00.4Z",
            "2025-07-26t12:00:00z",
        ] {
            assert_eq!(parse_time(text, &auto), Ok(NOON), "{text}");
        }
        assert_eq!(parse_time("2025-07-26T11:59:59.6Z", &auto), Ok(NOON));
        assert_eq!(parse_time("2025-07-26", &auto), Ok(NOON - 12 * 3600));
        assert_eq!(parse_time("1970-01-01T00:00:00Z", &auto), Ok(0));
        assert_eq!(parse_time("2000-02-29T00:00:00Z", &auto), Ok(951_782_400));
    }

    #[test]
    fn epoch_seconds_and_milliseconds() {
        assert_eq!(parse_time("1753531200", &TimeFormat::Auto), Ok(NOON));
        assert_eq!(parse_time("1753531200000", &TimeFormat::Auto), Ok(NOON));
        assert_eq!(parse_time("1753531200.4", &TimeFormat::Auto), Ok(NOON));
        assert_eq!(
            parse_time("1753531200000", &TimeFormat::EpochMillis),
            Ok(NOON)
        );
        assert_eq!(
            parse_time("1753531200", &TimeFormat::EpochSeconds),
            Ok(NOON)
        );
    }

    #[test]
    fn user_formats() {
        let f = |p: &str| TimeFormat::Custom(p.to_owned());
        assert_eq!(
            parse_time("26/07/2025 12:00:00", &f("%d/%m/%Y %H:%M:%S")),
            Ok(NOON)
        );
        assert_eq!(
            parse_time("26 Jul 25 12h00", &f("%d %b %y %Hh%M")),
            Ok(NOON)
        );
        assert_eq!(
            parse_time("26 July 2025 14:00 +02", &f("%d %b %Y %H:%M %z")),
            Ok(NOON)
        );
        assert_eq!(
            parse_time("20250726120000.5", &f("%Y%m%d%H%M%S.%f")),
            Ok(NOON + 1)
        );
        assert!(check_format("%d/%m/%Y").is_ok());
        assert!(check_format("%d/%m").is_err());
        assert!(check_format("%Y %q").is_err());
    }

    #[test]
    fn bad_times_are_refused_not_guessed() {
        for text in [
            "",
            "yesterday",
            "2025-13-01",
            "2025-02-30T00:00:00",
            "2025-07-26T12",
            "2025-07-26T12:00:00Zjunk",
            "NaN",
            "1e300",
        ] {
            assert!(parse_time(text, &TimeFormat::Auto).is_err(), "{text:?}");
        }
        assert!(parse_time("26/07/2025", &TimeFormat::Custom("%d-%m-%Y".to_owned())).is_err());
    }

    #[test]
    fn midnight_as_24_00_and_zone_offsets_within_14_hours() {
        let auto = TimeFormat::Auto;
        // 24:00 is the next day's midnight, and only exactly that.
        assert_eq!(
            parse_time("2025-07-25T24:00:00Z", &auto),
            Ok(NOON - 12 * 3600)
        );
        assert_eq!(
            parse_time("2025-07-25T24:00:00.0Z", &auto),
            Ok(NOON - 12 * 3600)
        );
        assert_eq!(parse_time("2025-07-25 24:00", &auto), Ok(NOON - 12 * 3600));
        for text in [
            "2025-07-25T24:00:01Z",
            "2025-07-25T24:01:00Z",
            "2025-07-25T24:00:00.5Z",
            "2025-07-25T25:00:00Z",
        ] {
            assert!(parse_time(text, &auto).is_err(), "{text}");
        }
        assert_eq!(parse_time("2025-07-27T02:00:00+14:00", &auto), Ok(NOON));
        assert_eq!(parse_time("2025-07-26T00:00:00-12:00", &auto), Ok(NOON));
        for text in [
            "2025-07-26T12:00:00+14:01",
            "2025-07-26T12:00:00+15:00",
            "2025-07-26T12:00:00-14:30",
            "2025-07-26T12:00:00+05:60",
        ] {
            assert!(parse_time(text, &auto).is_err(), "{text}");
        }
        let custom = TimeFormat::Custom("%Y-%m-%d %H:%M %z".to_owned());
        assert!(parse_time("2025-07-26 12:00 +99", &custom).is_err());
    }

    #[test]
    fn guessing_a_column() {
        assert_eq!(
            guess_format(["1753531200", "1753531260"]),
            Some(TimeFormat::EpochSeconds)
        );
        assert_eq!(
            guess_format(["1753531200000"]),
            Some(TimeFormat::EpochMillis)
        );
        assert_eq!(
            guess_format(["2025-07-26T12:00:00Z", " "]),
            Some(TimeFormat::Iso8601)
        );
        assert_eq!(guess_format(["26/07/2025 12:00"]), None);
        assert_eq!(guess_format([]), None);
    }
}
