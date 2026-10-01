//! CSV tracks (spec.md 7.3).
//!
//! A header row is required. Reading happens in two steps, as the
//! column-mapping dialog needs: [`parse_table`] splits the file into cells
//! (quoted fields, `,` `;` or tab, chosen from the header), [`guess`]
//! proposes which column is which from the header names and the first
//! values, and [`read_csv`] reads the fixes with the mapping the user
//! confirmed.

use pe_core::track::Fix;

use crate::error::{Reason, Result, TrackFileError};
use crate::time::{TimeFormat, guess_format, parse_time};
use crate::{MAX_FIXES, RawTrack, check_heading, check_speed, position};

/// One row of cells, with where each came from.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvRow {
    /// 1-based line the row starts on.
    pub line: usize,
    /// The cells, unquoted.
    pub cells: Vec<String>,
    /// 1-based column (in characters) each cell starts at.
    pub columns: Vec<usize>,
}

/// A CSV file split into cells.
#[derive(Debug, Clone, PartialEq)]
pub struct CsvTable {
    /// The field separator found in the header.
    pub delimiter: char,
    /// The header's names.
    pub header: Vec<String>,
    /// Every data row.
    pub rows: Vec<CsvRow>,
}

/// Unit of the speed column.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum SpeedUnit {
    /// Knots.
    #[default]
    Knots,
    /// Metres per second.
    MetresPerSecond,
    /// Kilometres per hour.
    KilometresPerHour,
    /// Statute miles per hour.
    MilesPerHour,
}

impl SpeedUnit {
    /// Knots per one of this unit.
    pub fn to_knots(self) -> f64 {
        match self {
            Self::Knots => 1.0,
            Self::MetresPerSecond => 3600.0 / 1852.0,
            Self::KilometresPerHour => 1000.0 / 1852.0,
            Self::MilesPerHour => 1609.344 / 1852.0,
        }
    }
}

/// Which column holds what: 0-based indices into the header.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CsvMapping {
    /// Time.
    pub time: usize,
    /// Latitude, decimal degrees.
    pub lat: usize,
    /// Longitude, decimal degrees.
    pub lon: usize,
    /// Heading or COG, degrees; optional.
    pub heading: Option<usize>,
    /// SOG or boat speed; optional.
    pub speed: Option<usize>,
    /// Supplied true wind speed column.
    pub tws: Option<usize>,
    /// Supplied true wind direction column, meteorological degrees from north.
    pub twd: Option<usize>,
    /// Boat name, for a file with several boats; optional.
    pub boat: Option<usize>,
    /// How the time column is written.
    pub time_format: TimeFormat,
    /// Unit of the speed column.
    pub speed_unit: SpeedUnit,
    /// Independent unit for the supplied wind speed column.
    pub wind_speed_unit: SpeedUnit,
}

/// What [`guess`] proposes: each column it could place, and the time format
/// the first values suggest.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CsvGuess {
    /// Time column.
    pub time: Option<usize>,
    /// Latitude column.
    pub lat: Option<usize>,
    /// Longitude column.
    pub lon: Option<usize>,
    /// Heading column.
    pub heading: Option<usize>,
    /// Speed column.
    pub speed: Option<usize>,
    /// Supplied true wind speed column.
    pub tws: Option<usize>,
    /// Supplied true wind direction column, meteorological degrees from north.
    pub twd: Option<usize>,
    /// Boat column.
    pub boat: Option<usize>,
    /// Time format, when the first values agree on one.
    pub time_format: Option<TimeFormat>,
    /// Speed unit, when the header names one.
    pub speed_unit: SpeedUnit,
    /// Independent unit for the supplied wind speed column.
    pub wind_speed_unit: SpeedUnit,
}

/// Splits one record starting at `start` (a byte offset), returning the
/// cells, their columns and the offset after the record's line end.
fn split_record(
    text: &str,
    start: usize,
    delimiter: char,
    line: &mut usize,
) -> Result<(Vec<String>, Vec<usize>, usize)> {
    let mut cells = Vec::new();
    let mut columns = Vec::new();
    let mut cell = String::new();
    let mut column = 1;
    let mut cell_column = 1;
    let mut quoted = false;
    let mut at_cell_start = true;
    let mut chars = text[start..].char_indices().peekable();
    let mut quote_open = (*line, 1);
    while let Some((offset, ch)) = chars.next() {
        if quoted {
            if ch == '"' {
                if chars.peek().map(|(_, c)| *c) == Some('"') {
                    chars.next();
                    column += 1;
                    cell.push('"');
                } else {
                    quoted = false;
                }
            } else {
                if ch == '\n' {
                    *line += 1;
                    column = 0;
                }
                cell.push(ch);
            }
            column += 1;
            continue;
        }
        match ch {
            '"' if at_cell_start => {
                quoted = true;
                quote_open = (*line, column);
                at_cell_start = false;
            }
            c if c == delimiter => {
                cells.push(std::mem::take(&mut cell));
                columns.push(cell_column);
                cell_column = column + 1;
                at_cell_start = true;
            }
            '\r' if chars.peek().map(|(_, c)| *c) == Some('\n') => {}
            '\n' => {
                cells.push(cell);
                columns.push(cell_column);
                *line += 1;
                return Ok((cells, columns, start + offset + 1));
            }
            c => {
                cell.push(c);
                at_cell_start = false;
            }
        }
        column += 1;
    }
    if quoted {
        return Err(TrackFileError::at(
            quote_open.0,
            quote_open.1,
            Reason::UnclosedQuote,
        ));
    }
    cells.push(cell);
    columns.push(cell_column);
    Ok((cells, columns, text.len()))
}

/// The separator the header uses most, outside quotes: `,`, `;` or tab.
fn detect_delimiter(header_line: &str) -> char {
    let mut counts = [(',', 0usize), (';', 0), ('\t', 0)];
    let mut quoted = false;
    for ch in header_line.chars() {
        if ch == '"' {
            quoted = !quoted;
        } else if !quoted && let Some(entry) = counts.iter_mut().find(|(c, _)| *c == ch) {
            entry.1 += 1;
        }
    }
    counts
        .iter()
        .max_by_key(|(_, n)| *n)
        .filter(|(_, n)| *n > 0)
        .map_or(',', |(c, _)| *c)
}

/// Splits a CSV file into its header and rows. Blank lines are skipped.
pub fn parse_table(text: &str) -> Result<CsvTable> {
    let text = text.strip_prefix('\u{feff}').unwrap_or(text);
    let mut line = 1;
    let mut offset = 0;
    // The header is the first non-blank line.
    while offset < text.len() {
        let end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
        if !text[offset..end].trim().is_empty() {
            break;
        }
        offset = (end + 1).min(text.len());
        line += 1;
    }
    if offset >= text.len() {
        return Err(TrackFileError::whole(Reason::Empty));
    }
    let header_end = text[offset..].find('\n').map_or(text.len(), |i| offset + i);
    let delimiter = detect_delimiter(&text[offset..header_end]);
    let (header, _, mut offset) = split_record(text, offset, delimiter, &mut line)?;
    let header: Vec<String> = header.into_iter().map(|h| h.trim().to_owned()).collect();
    if header.len() < 2 || header.iter().all(String::is_empty) {
        return Err(TrackFileError::at(1, 1, Reason::NoHeader));
    }
    let mut rows = Vec::new();
    while offset < text.len() {
        let row_line = line;
        let (cells, columns, next) = split_record(text, offset, delimiter, &mut line)?;
        offset = next;
        if cells.iter().all(|c| c.trim().is_empty()) {
            continue;
        }
        if rows.len() >= MAX_FIXES {
            return Err(TrackFileError::whole(Reason::TooManyFixes));
        }
        rows.push(CsvRow {
            line: row_line,
            cells,
            columns,
        });
    }
    Ok(CsvTable {
        delimiter,
        header,
        rows,
    })
}

/// A header name reduced to lower-case letters and digits.
fn normal(name: &str) -> String {
    name.chars()
        .filter(char::is_ascii_alphanumeric)
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

/// The column whose name is one of `exact`, else the first that starts with
/// one of `prefixes`.
fn find(
    names: &[String],
    exact: &[&str],
    prefixes: &[&str],
    taken: &[Option<usize>],
) -> Option<usize> {
    let free = |i: &usize| !taken.contains(&Some(*i));
    names
        .iter()
        .position(|n| exact.contains(&n.as_str()))
        .filter(free)
        .or_else(|| {
            (0..names.len())
                .filter(free)
                .find(|&i| prefixes.iter().any(|p| names[i].starts_with(p)))
        })
}

/// Proposes a mapping from the header names and the first values.
pub fn guess(table: &CsvTable) -> CsvGuess {
    let names: Vec<String> = table.header.iter().map(|h| normal(h)).collect();
    let mut g = CsvGuess {
        time: find(
            &names,
            &[
                "time",
                "timestamp",
                "datetime",
                "date",
                "utc",
                "gpstime",
                "timeutc",
                "utctime",
                "datetimeutc",
            ],
            &["time", "date", "utc"],
            &[],
        ),
        ..CsvGuess::default()
    };
    g.lat = find(&names, &["lat", "latitude", "y"], &["lat"], &[g.time]);
    g.lon = find(
        &names,
        &["lon", "lng", "long", "longitude", "x"],
        &["lon", "lng"],
        &[g.time, g.lat],
    );
    g.tws = find(
        &names,
        &["tws", "truewindspeed", "windspeed"],
        &["tws", "truewindspeed", "windspeed"],
        &[],
    );
    g.twd = find(
        &names,
        &["twd", "twdfrom", "truewinddirection", "winddirection"],
        &["twd", "truewinddirection", "winddirection"],
        &[],
    );
    g.heading = find(
        &names,
        &["cog", "heading", "hdg", "course", "cogt", "hdgt"],
        &["cog", "heading", "hdg", "course"],
        &[g.time, g.lat, g.lon],
    );
    g.speed = find(
        &names,
        &["sog", "speed", "bsp", "stw"],
        &["sog", "speed", "bsp", "stw"],
        &[g.time, g.lat, g.lon, g.heading, g.tws, g.twd],
    );
    g.boat = find(
        &names,
        &["boat", "name", "boatname", "yacht", "vessel"],
        &["boat", "yacht", "vessel"],
        &[g.time, g.lat, g.lon, g.heading, g.speed],
    );
    if let Some(time) = g.time {
        g.time_format = guess_format(
            table
                .rows
                .iter()
                .take(20)
                .filter_map(|row| row.cells.get(time).map(String::as_str)),
        );
    }
    if let Some(speed) = g.speed {
        let raw = table.header[speed].to_ascii_lowercase();
        g.speed_unit = if raw.contains("km/h") || raw.contains("kmh") || raw.contains("kph") {
            SpeedUnit::KilometresPerHour
        } else if raw.contains("m/s") || raw.contains("mps") {
            SpeedUnit::MetresPerSecond
        } else if raw.contains("mph") {
            SpeedUnit::MilesPerHour
        } else {
            SpeedUnit::Knots
        };
    }
    if let Some(speed) = g.tws {
        let raw = table.header[speed].to_ascii_lowercase();
        g.wind_speed_unit = if raw.contains("km/h") || raw.contains("kmh") || raw.contains("kph") {
            SpeedUnit::KilometresPerHour
        } else if raw.contains("m/s") || raw.contains("mps") {
            SpeedUnit::MetresPerSecond
        } else if raw.contains("mph") {
            SpeedUnit::MilesPerHour
        } else {
            SpeedUnit::Knots
        };
    }
    g
}

/// A decimal number, accepting a decimal comma when the file is not
/// comma-separated.
fn number(text: &str, delimiter: char) -> Option<f64> {
    let text = text.trim();
    let parsed = text.parse::<f64>().ok().or_else(|| {
        (delimiter != ',')
            .then(|| text.replacen(',', ".", 1).parse::<f64>().ok())
            .flatten()
    });
    parsed.filter(|v| v.is_finite())
}

/// Reads the fixes, one raw track per boat (in the order the boats first
/// appear), or one track when there is no boat column.
pub fn read_csv(table: &CsvTable, mapping: &CsvMapping) -> Result<Vec<RawTrack>> {
    let width = table.header.len();
    for column in [
        Some(mapping.time),
        Some(mapping.lat),
        Some(mapping.lon),
        mapping.heading,
        mapping.speed,
        mapping.tws,
        mapping.twd,
        mapping.boat,
    ]
    .into_iter()
    .flatten()
    {
        if column >= width {
            return Err(TrackFileError::at(1, 1, Reason::MissingColumn(column + 1)));
        }
    }
    if let TimeFormat::Custom(format) = &mapping.time_format {
        crate::time::check_format(format).map_err(|e| TrackFileError::whole(Reason::BadTime(e)))?;
    }
    let mut tracks: Vec<RawTrack> = Vec::new();
    for row in &table.rows {
        let cell = |i: usize| -> Result<(&str, usize)> {
            match (row.cells.get(i), row.columns.get(i)) {
                (Some(c), Some(col)) => Ok((c.as_str(), *col)),
                _ => Err(TrackFileError::at(
                    row.line,
                    row.columns.last().copied().unwrap_or(1),
                    Reason::ShortRow,
                )),
            }
        };
        let numeric = |i: usize| -> Result<f64> {
            let (text, col) = cell(i)?;
            number(text, table.delimiter).ok_or_else(|| {
                TrackFileError::at(row.line, col, Reason::NotANumber(text.to_owned()))
            })
        };
        let optional = |i: Option<usize>| -> Result<Option<f64>> {
            match i {
                None => Ok(None),
                Some(i) => match cell(i) {
                    Ok((text, _)) if text.trim().is_empty() => Ok(None),
                    Err(_) => Ok(None),
                    Ok(_) => numeric(i).map(Some),
                },
            }
        };
        let (time_text, time_col) = cell(mapping.time)?;
        let t = parse_time(time_text, &mapping.time_format)
            .map_err(|e| TrackFileError::at(row.line, time_col, Reason::BadTime(e)))?;
        let lat = numeric(mapping.lat)?;
        let lon = numeric(mapping.lon)?;
        let (lat, lon) = position(lat, lon).ok_or_else(|| {
            TrackFileError::at(
                row.line,
                cell(mapping.lat).map_or(1, |(_, c)| c),
                Reason::BadPosition,
            )
        })?;
        let at = |i: Option<usize>| i.and_then(|i| row.columns.get(i).copied()).unwrap_or(1);
        let cog = check_heading(optional(mapping.heading)?)
            .map_err(|r| TrackFileError::at(row.line, at(mapping.heading), r))?;
        let sog = check_speed(optional(mapping.speed)?.map(|v| v * mapping.speed_unit.to_knots()))
            .map_err(|r| TrackFileError::at(row.line, at(mapping.speed), r))?;
        let boat = mapping
            .boat
            .and_then(|i| row.cells.get(i))
            .map(|b| b.trim().to_owned())
            .filter(|b| !b.is_empty());
        let fix = Fix {
            tws: crate::check_wind_speed(
                optional(mapping.tws)?.map(|v| v * mapping.wind_speed_unit.to_knots()),
            )
            .map_err(|r| TrackFileError::at(row.line, at(mapping.tws), r))?,
            twd_from: check_heading(optional(mapping.twd)?)
                .map_err(|r| TrackFileError::at(row.line, at(mapping.twd), r))?,
            t,
            lat,
            lon,
            cog,
            sog,
        };
        match tracks.iter_mut().find(|track| track.boat == boat) {
            Some(track) => track.fixes.push(fix),
            None => tracks.push(RawTrack {
                boat,
                fixes: vec![fix],
            }),
        }
    }
    if tracks.is_empty() {
        return Err(TrackFileError::whole(Reason::NoFixes));
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: i64 = 1_753_531_200;

    #[test]
    fn wind_columns_have_independent_units_and_report_invalid_values() {
        let text = "time,lat,lon,SOG (kn),TWS (m/s),TWD\n0,1,2,6,5,360\n60,1,2,6,,90\n";
        let table = parse_table(text).unwrap();
        let g = guess(&table);
        assert_eq!((g.speed, g.tws, g.twd), (Some(3), Some(4), Some(5)));
        assert_eq!(g.wind_speed_unit, SpeedUnit::MetresPerSecond);
        assert_eq!(g.speed_unit, SpeedUnit::Knots);
        let m = CsvMapping {
            tws: g.tws,
            twd: g.twd,
            speed: g.speed,
            wind_speed_unit: g.wind_speed_unit,
            ..mapping(TimeFormat::Auto)
        };
        let raw = read_csv(&table, &m).unwrap();
        let f = &raw[0].fixes[0];
        assert_eq!(f.sog, Some(6.0));
        assert!((f.tws.unwrap() - 5.0 * 3600.0 / 1852.0).abs() < 1e-6);
        assert_eq!(f.twd_from, Some(0.0));
        assert_eq!(raw[0].fixes[1].tws, None);
        let bad = parse_table(&text.replace(",5,360", ",-5,360")).unwrap();
        let error = read_csv(&bad, &m).unwrap_err();
        assert_eq!(error.line, Some(2));
        assert_eq!(error.reason.code(), "out-of-range");
    }

    fn mapping(time_format: TimeFormat) -> CsvMapping {
        CsvMapping {
            time: 0,
            lat: 1,
            lon: 2,
            heading: None,
            speed: None,
            boat: None,
            time_format,
            speed_unit: SpeedUnit::Knots,
            tws: None,
            twd: None,
            wind_speed_unit: SpeedUnit::Knots,
        }
    }

    #[test]
    fn guesses_columns_from_the_header_and_the_format_from_the_values() {
        let text = "Boat;UTC Time;Latitude (deg);Longitude (deg);COG;SOG (km/h)\n\
                    Alpha;2025-07-26T12:00:00Z;50,1;-1,3;45;10\n";
        let table = parse_table(text).unwrap();
        assert_eq!(table.delimiter, ';');
        let g = guess(&table);
        assert_eq!(
            (g.boat, g.time, g.lat, g.lon, g.heading, g.speed),
            (Some(0), Some(1), Some(2), Some(3), Some(4), Some(5))
        );
        assert_eq!(g.time_format, Some(TimeFormat::Iso8601));
        assert_eq!(g.speed_unit, SpeedUnit::KilometresPerHour);

        let m = CsvMapping {
            time: 1,
            lat: 2,
            lon: 3,
            heading: Some(4),
            speed: Some(5),
            boat: Some(0),
            time_format: TimeFormat::Auto,
            speed_unit: g.speed_unit,
            tws: None,
            twd: None,
            wind_speed_unit: SpeedUnit::Knots,
        };
        let tracks = read_csv(&table, &m).unwrap();
        let fix = &tracks[0].fixes[0];
        assert_eq!(
            (fix.t, fix.lat, fix.lon, fix.cog),
            (NOON, 50.1, -1.3, Some(45.0))
        );
        // 10 km/h is 10,000 / 1,852 = 5.399568 kn.
        assert!((fix.sog.unwrap() - 5.399_568).abs() < 1e-6);
        assert_eq!(tracks[0].boat.as_deref(), Some("Alpha"));
    }

    #[test]
    fn quoted_fields_several_boats_and_blank_lines() {
        let text = "time,lat,lon,name\r\n\
                    1753531200,10,179.9,\"Alpha, the first\"\r\n\
                    \r\n\
                    1753531260,10,-179.9,\"Bravo \"\"B\"\"\"\r\n\
                    1753531320,10,-179.8,\"Alpha, the first\"\r\n";
        let table = parse_table(text).unwrap();
        assert_eq!(table.rows.len(), 3);
        assert_eq!(table.rows[1].line, 4);
        let g = guess(&table);
        assert_eq!(g.boat, Some(3));
        assert_eq!(g.time_format, Some(TimeFormat::EpochSeconds));
        let m = CsvMapping {
            boat: Some(3),
            ..mapping(TimeFormat::EpochSeconds)
        };
        let tracks = read_csv(&table, &m).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].boat.as_deref(), Some("Alpha, the first"));
        assert_eq!(tracks[0].fixes.len(), 2);
        assert_eq!(tracks[1].boat.as_deref(), Some("Bravo \"B\""));
    }

    #[test]
    fn a_user_time_format() {
        let table = parse_table("when\tlat\tlon\n26/07/2025 12:00\t50\t-1\n").unwrap();
        assert_eq!(table.delimiter, '\t');
        assert_eq!(guess(&table).time_format, None);
        let tracks = read_csv(
            &table,
            &mapping(TimeFormat::Custom("%d/%m/%Y %H:%M".to_owned())),
        )
        .unwrap();
        assert_eq!(tracks[0].fixes[0].t, NOON);
    }

    #[test]
    fn malformed_rows_name_their_line_and_column() {
        let table = parse_table("time,lat,lon\n1753531200,50,-1\n1753531260,north,-1\n").unwrap();
        let err = read_csv(&table, &mapping(TimeFormat::Auto)).unwrap_err();
        assert_eq!((err.line, err.column), (Some(3), Some(12)));
        assert_eq!(err.reason, Reason::NotANumber("north".to_owned()));

        let table = parse_table("time,lat,lon\nlater,50,-1\n").unwrap();
        let err = read_csv(&table, &mapping(TimeFormat::Auto)).unwrap_err();
        assert_eq!(
            (err.line, err.column, err.reason.code()),
            (Some(2), Some(1), "bad-time")
        );

        let table = parse_table("time,lat,lon\n1753531200,50\n").unwrap();
        let err = read_csv(&table, &mapping(TimeFormat::Auto)).unwrap_err();
        assert_eq!((err.line, err.reason.code()), (Some(2), "short-row"));

        let table = parse_table("time,lat,lon\n1753531200,91,0\n").unwrap();
        assert_eq!(
            read_csv(&table, &mapping(TimeFormat::Auto))
                .unwrap_err()
                .reason
                .code(),
            "bad-position"
        );

        let err = parse_table("time,lat,lon\n1753531200,\"50,0\n").unwrap_err();
        assert_eq!(
            (err.line, err.column, err.reason.code()),
            (Some(2), Some(12), "unclosed-quote")
        );

        assert_eq!(parse_table("\n\n").unwrap_err().reason, Reason::Empty);
        assert_eq!(
            parse_table("justonecolumn\n1\n").unwrap_err().reason,
            Reason::NoHeader
        );
        let table = parse_table("time,lat,lon\n").unwrap();
        assert_eq!(
            read_csv(&table, &mapping(TimeFormat::Auto))
                .unwrap_err()
                .reason,
            Reason::NoFixes
        );
        let table = parse_table("time,lat,lon\n0,0,0\n").unwrap();
        let wide = CsvMapping {
            boat: Some(7),
            ..mapping(TimeFormat::Auto)
        };
        assert_eq!(
            read_csv(&table, &wide).unwrap_err().reason,
            Reason::MissingColumn(8)
        );
    }
}
