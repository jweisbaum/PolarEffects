//! An ORC certificate record as plain data.
//!
//! `pe-orc` builds and searches the embedded catalogue; adding a boat copies
//! its record into the project as a source (spec.md 5.3), so the record type
//! is part of the document and lives here. Fields are what the catalogue
//! builder keeps from jieter/orc-data (spec.md 5.1); anything a certificate
//! may lack is optional.

use serde::{Deserialize, Serialize};

use crate::canonical;

/// One boat's certificate.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrcRecord {
    /// Certificate or reference number, when orc-data gives one. Lets the app
    /// ask before the same certificate is added twice.
    #[serde(default)]
    pub ref_no: Option<String>,
    /// Sail number as printed, e.g. `"GBR 1124"`.
    pub sail_no: String,
    /// Three-letter country code.
    pub country: String,
    /// Boat name.
    pub name: String,
    /// Type or model, e.g. `"Farr 40"`.
    #[serde(default)]
    pub model: Option<String>,
    /// Builder.
    #[serde(default)]
    pub builder: Option<String>,
    /// Designer.
    #[serde(default)]
    pub designer: Option<String>,
    /// Year built.
    #[serde(default)]
    pub year: Option<i32>,
    /// Year of the certificate the VPP comes from. orc-data does not state
    /// it; the catalogue builder infers it from the VPP's wind-speed axis and
    /// the yearly lists (spec.md 5.1), and leaves it empty when it cannot.
    #[serde(default)]
    pub certificate_year: Option<i32>,
    /// Size fields.
    #[serde(default)]
    pub size: OrcSize,
    /// General purpose handicap, seconds per mile.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub gph: Option<f64>,
    /// Offshore single number, seconds per mile.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub osn: Option<f64>,
    /// The velocity prediction.
    pub vpp: OrcVpp,
}

/// A certificate's size fields. Lengths in metres, areas in square metres,
/// displacement in kilograms.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrcSize {
    /// Length overall, m.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub loa: Option<f64>,
    /// Beam, m.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub beam: Option<f64>,
    /// Draft, m.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub draft: Option<f64>,
    /// Displacement, kg.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub displacement_kg: Option<f64>,
    /// Mainsail area, m².
    #[serde(default, with = "canonical::optional_metres_field")]
    pub main_area: Option<f64>,
    /// Largest headsail area, m².
    #[serde(default, with = "canonical::optional_metres_field")]
    pub genoa_area: Option<f64>,
    /// Symmetric spinnaker area, m².
    #[serde(default, with = "canonical::optional_metres_field")]
    pub spinnaker_area: Option<f64>,
    /// Asymmetric spinnaker area, m².
    #[serde(default, with = "canonical::optional_metres_field")]
    pub asym_spinnaker_area: Option<f64>,
    /// Crew weight, kg.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub crew_kg: Option<f64>,
}

/// The ORC VPP table, kept exactly as the certificate gives it.
///
/// The speed axis differs by year (6–24 kn up to 2024, 4–24 kn from 2025);
/// both are kept as given (spec.md 5.1). Conversion to a polar grid is
/// `pe-polar`'s business.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct OrcVpp {
    /// True wind angles of the table, degrees.
    #[serde(with = "canonical::degrees_list")]
    pub angles: Vec<f64>,
    /// True wind speeds of the table, knots.
    #[serde(with = "canonical::knots_list")]
    pub speeds: Vec<f64>,
    /// Boat speed per angle (rows) per wind speed (columns), knots.
    #[serde(with = "canonical::optional_knots_rows")]
    pub bsp: Vec<Vec<Option<f64>>>,
    /// Optimum beat angle per wind speed, degrees.
    #[serde(with = "canonical::degrees_list")]
    pub beat_angle: Vec<f64>,
    /// Beat VMG per wind speed, knots.
    #[serde(with = "canonical::knots_list")]
    pub beat_vmg: Vec<f64>,
    /// Optimum run angle per wind speed, degrees.
    #[serde(with = "canonical::degrees_list")]
    pub run_angle: Vec<f64>,
    /// Run VMG per wind speed, knots.
    #[serde(with = "canonical::knots_list")]
    pub run_vmg: Vec<f64>,
}

/// The sail number as the catalogue shows it, from the number with its
/// separators removed, optionally behind `"<country>/"`: `"GBR/GBR1124"` and
/// `"GBR1124"` are `"GBR 1124"`. A stand-in for a missing number (`"_3"`) and
/// an empty one are empty.
///
/// One function for the catalogue builder and the ORC scraper, so the same
/// certificate reads the same from either and is recognised as one
/// (spec.md 5.4).
pub fn sail_display(sailnumber: &str, country: &str) -> String {
    let raw = sailnumber
        .split_once('/')
        .map_or(sailnumber, |(_, sail)| sail)
        .trim();
    if raw.is_empty() || raw.starts_with('_') {
        return String::new();
    }
    let rest = raw
        .get(..country.len())
        .filter(|head| !country.is_empty() && head.eq_ignore_ascii_case(country))
        .and_then(|_| raw.get(country.len()..))
        .map(|rest| rest.trim_start_matches([' ', '-', '/']))
        .filter(|rest| !rest.is_empty())
        .unwrap_or(raw);
    if country.is_empty() {
        rest.to_owned()
    } else {
        format!("{country} {rest}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sail_numbers_are_shown_once_with_their_country() {
        assert_eq!(sail_display("GBR/GBR1124", "GBR"), "GBR 1124");
        assert_eq!(sail_display("GBR/1124", "GBR"), "GBR 1124");
        assert_eq!(sail_display("AUS/Sm35", "AUS"), "AUS Sm35");
        assert_eq!(sail_display("FIN/Fin71", "FIN"), "FIN 71");
        assert_eq!(sail_display("GBR/_1", "GBR"), "");
        assert_eq!(sail_display("GBR/GBR", "GBR"), "GBR GBR");
        // Without the country in front, as the scraper has it.
        assert_eq!(sail_display("NOR14438", "NOR"), "NOR 14438");
        assert_eq!(sail_display("", "NOR"), "");
    }
}
