//! Expedition polars (`.txt`, spec.md 6): `!` comments, then one row per
//! wind speed, `TWS` followed by `TWA BSP` pairs, whitespace-separated. Rows
//! may have different lengths and different angles; the polar keeps each
//! row's own points on a union TWA axis, leaving the other cells empty.
//!
//! A leading label row (`twa0 bsp0 TwaUp bspUp …`, naming the columns
//! instead of holding a wind speed) is skipped rather than read as data.

use std::collections::BTreeSet;

use pe_core::polar::PolarGrid;

use crate::build::Builder;
use crate::format::{
    PolarError, Reason, Result, Separator, angle, axis_text, bsp_text, fields,
    is_expedition_label_row, meaningful, speed, tws_speed,
};

/// The comment line the writer starts with.
const HEADER: &str = "!Expedition polar: TWS, then TWA and BSP pairs (knots, degrees)";

pub(crate) fn read(text: &str) -> Result<PolarGrid> {
    let sep = Separator::Whitespace;
    let mut builder = Builder::default();
    let mut seen_tws = BTreeSet::new();
    let mut lines = meaningful(text);
    let mut first_row = lines.next();
    if let Some((_, content)) = first_row
        && is_expedition_label_row(&fields(content, sep))
    {
        first_row = lines.next();
    }
    for (line, content) in first_row.into_iter().chain(lines) {
        let row = fields(content, sep);
        let Some((first, pairs)) = row.split_first() else {
            continue;
        };
        let tws = tws_speed(line, *first, sep)?;
        if !seen_tws.insert(tws.to_bits()) {
            return Err(PolarError::at(
                line,
                first.column,
                Reason::DuplicateTws(tws),
            ));
        }
        if pairs.is_empty() {
            // A wind speed with no points says nothing.
            continue;
        }
        let tws_key = builder
            .tws(tws)
            .map_err(|reason| PolarError::at(line, first.column, reason))?;
        let mut seen_twa = BTreeSet::new();
        for pair in pairs.chunks(2) {
            let [twa_field, bsp_field] = pair else {
                return Err(PolarError::at(line, pair[0].column, Reason::MissingBsp));
            };
            let raw = angle(line, *twa_field, sep)?;
            if !seen_twa.insert(raw.to_bits()) {
                return Err(PolarError::at(
                    line,
                    twa_field.column,
                    Reason::DuplicateTwa(raw),
                ));
            }
            let bsp = speed(line, *bsp_field, sep)?;
            // Retain both sides so an asymmetric export reads back losslessly.
            let twa_key = builder
                .twa(raw)
                .map_err(|reason| PolarError::at(line, twa_field.column, reason))?;
            builder.cell(twa_key, tws_key, bsp);
        }
    }
    if !builder.has_cells() {
        return Err(PolarError::whole(if text.trim().is_empty() {
            Reason::Empty
        } else {
            Reason::NoSpeeds
        }));
    }
    Ok(builder.finish())
}

/// One line per wind speed that has a value, tab-separated, with only the
/// cells that hold one: what an Expedition polar can say, and nothing more.
pub(crate) fn write(polar: &PolarGrid) -> String {
    let mut out = String::from(HEADER);
    out.push('\n');
    for (j, tws) in polar.tws.iter().enumerate() {
        let mut row = vec![axis_text(*tws)];
        for (i, twa) in polar.twa.iter().enumerate() {
            if let Some(bsp) = polar.get(i, j) {
                row.push(axis_text(*twa));
                row.push(bsp_text(bsp));
            }
        }
        if row.len() > 1 {
            out.push_str(&row.join("\t"));
            out.push('\n');
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn err(text: &str) -> PolarError {
        read(text).unwrap_err()
    }

    #[test]
    fn rows_keep_their_own_angles() {
        let polar = read("! comment\n\n8 45 6 90 7.5\r\n6\t40  5   90 6.5\n").unwrap();
        assert_eq!(polar.tws, [6.0, 8.0]);
        assert_eq!(polar.twa, [40.0, 45.0, 90.0]);
        assert_eq!(
            polar.bsp,
            [
                vec![Some(5.0), None],
                vec![None, Some(6.0)],
                vec![Some(6.5), Some(7.5)]
            ]
        );
    }

    #[test]
    fn a_leading_label_row_is_skipped_not_read_as_data() {
        // Swan 78.txt shaped: a tab-led label row, then data rows.
        let polar = read("\ttwa0\tbsp0\tTwaUp\tbspUp\n6\t0\t0\t45\t7.04\n").unwrap();
        assert_eq!(polar.tws, [6.0]);
        assert_eq!(polar.twa, [0.0, 45.0]);
        assert_eq!(polar.bsp, [vec![Some(0.0)], vec![Some(7.04)]]);

        // J35.txt shaped: space-separated, starting with a word, not a tab.
        let polar = read("pol  Twa0  Bsp0  UpTwa  UpBsp\n6.3  30  0  45.1  4.95\n").unwrap();
        assert_eq!(polar.tws, [6.3]);
        assert_eq!(polar.twa, [30.0, 45.1]);
        assert_eq!(polar.bsp, [vec![Some(0.0)], vec![Some(4.95)]]);
    }

    #[test]
    fn port_angles_keep_independent_values() {
        let polar = read("6 40 5 320 5.5 200 6\n").unwrap();
        assert_eq!(polar.twa, [40.0, 200.0, 320.0]);
        assert_eq!(
            polar.bsp,
            [vec![Some(5.0)], vec![Some(6.0)], vec![Some(5.5)]]
        );
    }

    #[test]
    fn errors_name_line_and_column() {
        let e = err("!x\n6 40 5\n8 40 abc\n");
        assert_eq!((e.line, e.column), (3, 6));
        assert_eq!(e.reason, Reason::NotANumber("abc".into()));

        let e = err("6 40 5 90\n");
        assert_eq!((e.line, e.column, e.reason), (1, 8, Reason::MissingBsp));

        let e = err("6 40 61\n");
        assert_eq!((e.column, e.reason), (6, Reason::TooFast(61.0)));
        let e = err("6 40 -1\n");
        assert_eq!((e.column, e.reason), (6, Reason::Negative(-1.0)));
        // TWS may run to 70 kn (a gale, not a boat speed); 71 is refused.
        assert_eq!(read("70 40 1\n").unwrap().tws, [70.0]);
        let e = err("71 40 1\n");
        assert_eq!((e.column, e.reason), (1, Reason::TooFast(71.0)));
        let e = err("6 400 1\n");
        assert_eq!((e.column, e.reason), (3, Reason::AngleOutOfRange(400.0)));
        let e = err("6 40 1\n6 50 2\n");
        assert_eq!((e.line, e.reason), (2, Reason::DuplicateTws(6.0)));
        let e = err("6 40 1 40 2\n");
        assert_eq!((e.column, e.reason), (8, Reason::DuplicateTwa(40.0)));
        assert_eq!(err("6\n8\n").reason, Reason::NoSpeeds);
        assert_eq!(err("6 inf 5\n").reason, Reason::NotANumber("inf".into()));
    }

    #[test]
    fn writing_keeps_only_what_the_rows_hold() {
        let polar = read("6 40 5 90 6.5\n8 45 6 90 7.5\n").unwrap();
        assert_eq!(
            write(&polar),
            format!("{HEADER}\n6\t40\t5.00\t90\t6.50\n8\t45\t6.00\t90\t7.50\n")
        );
        assert_eq!(read(&write(&polar)).unwrap(), polar);
    }
}
