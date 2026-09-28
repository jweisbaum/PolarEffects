#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Polar files against golden bytes (spec.md 6, invariant 5).
//!
//! The golden files in `tests/golden/` were written by hand to the format
//! rules — not produced by the writer — and pin its output: axes with at most
//! two decimals, boat speeds with exactly two, tabs (Expedition, Adrena) or
//! semicolons (CSV), `\n` endings. Changing them is changing export output:
//! see "Changing export output" in CLAUDE.md.
//!
//! The fixtures in `tests/fixtures/` are the same polars as a person's files
//! arrive: CRLF, BOMs, UTF-16, quoted cells, decimal commas, trailing
//! separators, shuffled rows, port-side angles and every header variant.

use pe_polar::format::{MAX_AXIS_VALUES, MAX_FILE_BYTES};
use pe_polar::{Polar, PolarError, PolarFileFormat, Reason, cell_count, read, write};
use proptest::prelude::*;

const EXPEDITION: &[u8] = include_bytes!("golden/expedition.txt");
const ADRENA: &[u8] = include_bytes!("golden/adrena.pol");
const CSV: &[u8] = include_bytes!("golden/grid.csv");

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(format!(
        "{}/tests/fixtures/{name}",
        env!("CARGO_MANIFEST_DIR")
    ))
    .unwrap()
}

fn cell(polar: &Polar, twa: f64, tws: f64) -> Option<f64> {
    let i = polar.twa.iter().position(|a| *a == twa)?;
    let j = polar.tws.iter().position(|s| *s == tws)?;
    polar.get(i, j)
}

/// Golden → read → write gives the golden bytes back, and reading what was
/// written gives the same polar.
fn round_trip(golden: &[u8], format: PolarFileFormat) -> Polar {
    let parsed = read(golden).unwrap();
    assert_eq!(parsed.format, format);
    parsed.polar.validate().unwrap();
    let written = write(format, &parsed.polar);
    assert_eq!(
        written,
        std::str::from_utf8(golden).unwrap(),
        "{format:?} output drifted from its golden file"
    );
    assert_eq!(read(written.as_bytes()).unwrap().polar, parsed.polar);
    parsed.polar
}

#[test]
fn expedition_round_trips_through_its_golden_file() {
    let polar = round_trip(EXPEDITION, PolarFileFormat::Expedition);
    assert_eq!(polar.tws, [6.0, 10.0, 16.0, 25.0]);
    // Each wind speed keeps its own beat and run angles: a union axis of the
    // nine shared angles and eight that belong to one row each.
    assert_eq!(polar.twa.len(), 9 + 8);
    assert_eq!(cell_count(&polar), 4 * 11);
    assert_eq!(cell(&polar, 41.8, 10.0), Some(5.95));
    assert_eq!(cell(&polar, 41.8, 6.0), None);
    assert_eq!(cell(&polar, 39.25, 16.0), Some(6.45));
    assert_eq!(cell(&polar, 120.0, 25.0), Some(10.05));
    assert_eq!(cell(&polar, 0.0, 25.0), Some(0.0));
}

#[test]
fn adrena_round_trips_through_its_golden_file() {
    let polar = round_trip(ADRENA, PolarFileFormat::Adrena);
    assert_eq!(polar.tws, [6.0, 8.0, 10.0, 12.0, 16.0, 20.0]);
    assert_eq!(polar.twa.len(), 12);
    // The one empty cell of the file stays empty.
    assert_eq!(cell(&polar, 42.5, 6.0), None);
    assert_eq!(cell_count(&polar), 12 * 6 - 1);
    assert_eq!(cell(&polar, 90.0, 12.0), Some(8.77));
    assert_eq!(cell(&polar, 180.0, 20.0), Some(9.27));
}

#[test]
fn csv_round_trips_through_its_golden_file() {
    let polar = round_trip(CSV, PolarFileFormat::Csv);
    // The same polar as the Adrena file, in the other layout.
    assert_eq!(polar, read(ADRENA).unwrap().polar);
    assert_eq!(
        write(PolarFileFormat::Csv, &read(ADRENA).unwrap().polar).as_bytes(),
        CSV
    );
}

#[test]
fn messy_files_read_as_the_golden_polars() {
    let expedition = read(&fixture("expedition-messy.txt")).unwrap();
    assert_eq!(expedition.format, PolarFileFormat::Expedition);
    assert_eq!(expedition.polar, read(EXPEDITION).unwrap().polar);
    // 185° and 250° were folded onto 175° and 110°.
    assert_eq!(cell(&expedition.polar, 175.0, 25.0), Some(8.25));
    assert_eq!(cell(&expedition.polar, 110.0, 10.0), Some(7.6));
    assert_eq!(
        write(PolarFileFormat::Expedition, &expedition.polar).as_bytes(),
        EXPEDITION
    );

    let golden = read(ADRENA).unwrap().polar;
    for (name, format) in [
        ("adrena-slash.pol", PolarFileFormat::Adrena),
        ("grid-semicolon.csv", PolarFileFormat::Csv),
        ("grid-comma.csv", PolarFileFormat::Csv),
    ] {
        let parsed = read(&fixture(name)).unwrap();
        assert_eq!(parsed.format, format, "{name}");
        assert_eq!(parsed.polar, golden, "{name}");
        assert_eq!(
            write(format, &parsed.polar).as_bytes(),
            if format == PolarFileFormat::Csv {
                CSV
            } else {
                ADRENA
            }
        );
    }
}

#[test]
fn malformed_files_are_refused_at_their_line_and_column() {
    let cases: &[(&str, usize, usize, &str)] = &[
        ("", 1, 1, "empty"),
        ("!only a comment\n\n", 1, 1, "empty"),
        ("Boat speed table\n", 1, 1, "unknown-format"),
        ("!c\n6 40 5\n8 40 5.2 90\n", 3, 10, "missing-bsp"),
        ("6 40 5\n8 40 x\n", 2, 6, "not-a-number"),
        ("6 40 5\n8 40 60.5\n", 2, 6, "too-fast"),
        ("6 40 -5\n", 1, 6, "negative"),
        ("6 -40 5\n", 1, 3, "negative"),
        ("6 40 5\n6 50 5\n", 2, 1, "duplicate-tws"),
        ("6 40 5 361 5\n", 1, 8, "angle-out-of-range"),
        ("TWA\\TWS\t6\t8\n40\t5\t5.5\t6\n", 2, 10, "too-many-cells"),
        ("TWA\\TWS\t6\t61\n", 1, 11, "too-fast"),
        ("TWA/TWS;6\n40;5\n40;5\n", 3, 1, "duplicate-twa"),
        ("TWA;6;8\n\n  90;5;NaN\n", 3, 8, "not-a-number"),
        ("TWA;6\n", 1, 1, "no-speeds"),
    ];
    for (text, line, column, code) in cases {
        let err: PolarError = read(text.as_bytes()).unwrap_err();
        assert_eq!(
            (err.line, err.column, err.reason.code()),
            (*line, *column, *code),
            "{text:?}: {err}"
        );
    }
}

#[test]
fn hostile_sizes_are_refused_before_they_cost_anything() {
    let big = vec![b' '; MAX_FILE_BYTES + 1];
    assert_eq!(read(&big).unwrap_err().reason, Reason::TooLarge);

    // One row per wind speed, each with its own angle: the union axis would
    // grow past the limit.
    let mut text = String::new();
    for i in 0..=MAX_AXIS_VALUES {
        let tws = i as f64 * 0.1;
        let twa = i as f64 * 0.3;
        text.push_str(&format!("{tws:.1} {twa:.1} 5\n"));
    }
    assert_eq!(
        read(text.as_bytes()).unwrap_err().reason,
        Reason::TooManyValues
    );
}

fn check(bytes: &[u8]) {
    match read(bytes) {
        Ok(parsed) => parsed.polar.validate().unwrap(),
        Err(err) => assert!(err.line >= 1 && err.column >= 1, "{err}"),
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 512, ..ProptestConfig::default() })]

    /// Arbitrary bytes never panic, and whatever reads is a valid polar.
    #[test]
    fn arbitrary_bytes_never_panic(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        check(&bytes);
    }

    /// Nor do near-misses of real files: a golden file with a few bytes
    /// changed, inserted or dropped.
    #[test]
    fn damaged_files_never_panic(
        which in 0usize..3,
        edits in proptest::collection::vec((any::<usize>(), any::<u8>(), 0u8..3), 1..8),
    ) {
        let mut bytes = [EXPEDITION, ADRENA, CSV][which].to_vec();
        for (at, byte, op) in edits {
            let at = at % (bytes.len() + 1);
            match op {
                0 if at < bytes.len() => bytes[at] = byte,
                1 => bytes.insert(at, byte),
                _ if at < bytes.len() => { bytes.remove(at); }
                _ => {}
            }
        }
        check(&bytes);
    }

    /// Text made of the characters polar files are made of, in any order.
    #[test]
    fn polar_shaped_text_never_panics(text in "[TWAS0-9\\\\/.,;\t \r\n!\"-]{0,300}") {
        check(text.as_bytes());
    }
}
