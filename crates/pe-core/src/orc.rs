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
    /// Year of the certificate the VPP comes from.
    pub certificate_year: i32,
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
