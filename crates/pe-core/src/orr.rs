//! Public ORR certificate data copied into a project at import.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

use crate::{orc::OrcSize, polar::PolarGrid};

/// One certificate's offshore or short-course boat-speed table. The catalogue
/// key is the certificate SKU and variant, independent of list order or names.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrrRecord {
    /// RegattaMan's stable certificate SKU.
    pub sku: String,
    /// `offshore` or `short_course`.
    pub variant: String,
    /// Certificate number as printed in the valid list.
    pub certificate: String,
    /// Certificate year.
    pub year: i32,
    /// Boat name.
    pub name: String,
    /// Sail number as printed.
    pub sail_no: String,
    /// Boat type or model.
    pub model: Option<String>,
    /// Physical measurements in metres, square metres and kilograms.
    pub size: OrcSize,
    /// Boat speeds in knots, including optimum beat and run points.
    pub polar: PolarGrid,
    /// Complete public certificate payload. Older polar-only imports omit it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub details: Option<Box<OrrCertificateData>>,
}

/// Public certificate facts, kept separately from the knots used for blending.
/// Text preserves the publisher's precision, flags, blank values and dates;
/// dimensional originals use metric units even when the website displays feet.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrrCertificateData {
    /// Public page from which this immutable copy was read.
    pub source_url: String,
    /// Every named valid-list column, including effective/expiry dates and
    /// spin/non-spin IR and BM-PHRF ratings. Keys are the vendor column names.
    pub list_fields: BTreeMap<String, String>,
    /// All certificate data, performance metrics and line-drawing parameters,
    /// keyed by their vendor field identifier (for example `P-bcd`).
    pub fields: BTreeMap<String, OrrField>,
    /// Every published rating, grouped by its certificate section. Time
    /// allowances and correction factors must never be used as boat speeds.
    pub ratings: BTreeMap<String, Vec<OrrRating>>,
    /// Original boat-speed and time-allowance tables for both configurations,
    /// with their row labels, wind columns, units and footnotes intact.
    pub tables: BTreeMap<String, OrrTable>,
}

/// One named certificate datum, including values printed without an input.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrrField {
    /// Original group identifier, such as `rig` or `sail_trim`.
    pub section: String,
    /// Human-readable heading (the field identifier if no heading is printed).
    pub label: String,
    /// Original value, before any browser unit conversion.
    pub value: String,
    /// Printed enum label, when different from its stored code.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display: Option<String>,
    /// Vendor quantity type: length, weight, area, force, date, text, etc.
    pub quantity: String,
}

/// A rating from the page's embedded public JSON, before display rounding.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrrRating {
    /// Rating system identifier, including its precision and wind scheme.
    pub rsys: String,
    /// TCF, TOD or PCS (kept open for other published rating types).
    pub rtype: String,
    /// Course/race name.
    pub course: String,
    /// Wind band or speed; blank for an all-wind rating.
    pub wind: String,
    /// Spinnaker rating; missing remains missing, never zero.
    pub spin: Option<String>,
    /// Non-spinnaker rating, if published.
    pub nonspin: Option<String>,
    /// Preserve additional vendor attributes instead of silently dropping them.
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

/// A labeled published table. Strings retain precision, commas and unit labels.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrrTable {
    /// Knots for speed tables; seconds per nautical mile for allowances.
    pub unit: String,
    /// Header labels, starting with the row-heading column.
    pub columns: Vec<String>,
    /// Each row starts with its original label, followed by one value per wind.
    pub rows: Vec<Vec<String>>,
    /// The publisher's explanatory notes, including VMG/gybing assumptions.
    pub notes: Vec<String>,
}

impl OrrRecord {
    /// A repeatable key for deduplication and search-result selection.
    pub fn key(&self) -> String {
        format!("{}:{}", self.sku, self.variant)
    }

    /// A certificate's named original datum, if this is a complete scrape.
    pub fn field(&self, name: &str) -> Option<&str> {
        self.details
            .as_ref()?
            .fields
            .get(name)
            .map(|f| f.value.as_str())
    }

    /// Build year from the published ISO build date (not the certificate year).
    pub fn build_year(&self) -> Option<i32> {
        self.field("build_date-bt")?
            .split('-')
            .next()?
            .parse()
            .ok()
            .filter(|year| *year > 0)
    }
}
