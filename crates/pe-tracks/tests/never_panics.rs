//! Parsers return an error on any input; they never panic (CLAUDE.md,
//! testing rules).

use pe_tracks::csv::{CsvMapping, SpeedUnit, parse_table, read_csv};
use pe_tracks::geojson::read_geojson;
use pe_tracks::time::{TimeFormat, parse_time};
use proptest::prelude::*;

proptest! {
    #[test]
    fn geojson_never_panics(bytes in proptest::collection::vec(any::<u8>(), 0..512)) {
        let _ = read_geojson(&bytes);
    }

    #[test]
    fn geojson_like_text_never_panics(text in r#"\{"type":"Feature(Collection)?"[ -~]{0,200}\}"#) {
        let _ = read_geojson(text.as_bytes());
    }

    #[test]
    fn csv_never_panics(text in "[a-z0-9,;\t\"\r\n .:/T+-]{0,400}") {
        if let Ok(table) = parse_table(&text) {
            let width = table.header.len();
            let mapping = CsvMapping {
                time: 0,
                lat: 1 % width,
                lon: 2 % width,
                heading: Some(3),
                speed: Some(4),
                boat: None,
                time_format: TimeFormat::Auto,
                speed_unit: SpeedUnit::MetresPerSecond,
            };
            let _ = read_csv(&table, &mapping);
        }
    }

    #[test]
    fn times_never_panic(text in "\\PC{0,40}", format in "[%YymdHMSfbz: /T-]{0,12}") {
        let _ = parse_time(&text, &TimeFormat::Auto);
        let _ = parse_time(&text, &TimeFormat::Custom(format));
    }
}
