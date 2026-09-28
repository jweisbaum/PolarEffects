//! Sources and the overlays stored beside them (spec.md 4.1, 8).
//!
//! A source's data (`kind`) is immutable once imported. Everything the user
//! changes — colour, visibility, weight, label, exclusions, cell overrides,
//! filters — is stored beside it, and removing every overlay gives the source
//! back exactly (invariant 1).

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{CoreError, Result};
use crate::id::{SampleId, SourceId};
use crate::orc::OrcRecord;
use crate::polar::{PolarFileFormat, PolarGrid};
use crate::track::Track;

/// The weight a new source starts with.
pub const DEFAULT_WEIGHT: f64 = 1.0;
/// The largest weight a source may have (spec.md 8).
pub const MAX_WEIGHT: f64 = 2.0;

/// The palette new sources take colours from, in order (spec.md 8).
pub const PALETTE: [&str; 16] = [
    "#4e79a7", "#f28e2b", "#e15759", "#76b7b2", "#59a14f", "#edc948", "#b07aa1", "#ff9da7",
    "#9c755f", "#bab0ac", "#1f77b4", "#d62728", "#2ca02c", "#9467bd", "#17becf", "#bcbd22",
];

/// A colour as `#rrggbb`, lower case.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "String", into = "String")]
pub struct Colour(String);

impl Colour {
    /// Parses `#rrggbb` (either case), storing it lower case.
    pub fn parse(text: &str) -> Result<Self> {
        let valid = text.len() == 7
            && text.starts_with('#')
            && text[1..].chars().all(|c| c.is_ascii_hexdigit());
        if valid {
            Ok(Self(text.to_ascii_lowercase()))
        } else {
            Err(CoreError::Invalid(format!(
                "{text:?} is not a colour of the form #rrggbb"
            )))
        }
    }

    /// A colour from a literal known to be lower-case `#rrggbb` (the palette,
    /// the defaults); tested in this module.
    pub(crate) fn trusted(text: &str) -> Self {
        Self(text.to_owned())
    }

    /// The colour as `#rrggbb`.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<String> for Colour {
    type Error = CoreError;
    fn try_from(value: String) -> Result<Self> {
        Self::parse(&value)
    }
}

impl From<Colour> for String {
    fn from(value: Colour) -> Self {
        value.0
    }
}

impl std::fmt::Display for Colour {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// Anything that contributes to the polar.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Source {
    /// Stable id, allocated by the project.
    pub id: SourceId,
    /// The label shown in the source list; renamed in place.
    pub label: String,
    /// The source's colour everywhere it is drawn. Stored, never derived from
    /// list order.
    pub colour: Colour,
    /// Hidden sources are excluded from the blend and from every plot (D15).
    pub visible: bool,
    /// Blend weight in [0, 2].
    #[serde(with = "canonical::ratio_field")]
    pub weight: f64,
    /// The imported data. Immutable.
    pub kind: SourceKind,
    /// Every user change to it.
    #[serde(default)]
    pub overlay: Overlay,
}

impl Source {
    /// A visible source at the default weight with an empty overlay.
    pub fn new(id: SourceId, label: impl Into<String>, colour: Colour, kind: SourceKind) -> Self {
        Self {
            id,
            label: label.into(),
            colour,
            visible: true,
            weight: DEFAULT_WEIGHT,
            kind,
            overlay: Overlay::default(),
        }
    }

    /// The track, if this is a track source.
    pub fn track(&self) -> Option<&Track> {
        match &self.kind {
            SourceKind::Track { track } => Some(track),
            _ => None,
        }
    }

    /// The track, if this is a track source, mutably.
    pub fn track_mut(&mut self) -> Option<&mut Track> {
        match &mut self.kind {
            SourceKind::Track { track } => Some(track),
            _ => None,
        }
    }

    /// Checks the source's own rules.
    pub fn validate(&self) -> Result<()> {
        validate_weight(self.weight)?;
        match &self.kind {
            SourceKind::Orc { record } => {
                let vpp = &record.vpp;
                if vpp.bsp.len() != vpp.angles.len()
                    || vpp.bsp.iter().any(|row| row.len() != vpp.speeds.len())
                {
                    return Err(CoreError::Invalid(format!(
                        "the ORC table of {} does not match its axes",
                        self.label
                    )));
                }
            }
            SourceKind::PolarFile { polar, .. } => polar.validate()?,
            SourceKind::Track { track } => {
                for fix in &track.fixes {
                    if !(-90.0..=90.0).contains(&fix.lat) || !(-180.0..=180.0).contains(&fix.lon) {
                        return Err(CoreError::Invalid(format!(
                            "track {} has a fix at {}, {} which is not on Earth",
                            self.label, fix.lat, fix.lon
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

/// Checks a blend weight.
pub fn validate_weight(weight: f64) -> Result<()> {
    if weight.is_finite() && (0.0..=MAX_WEIGHT).contains(&weight) {
        Ok(())
    } else {
        Err(CoreError::Invalid(format!(
            "weight {weight} is outside 0..={MAX_WEIGHT}"
        )))
    }
}

/// What a source is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SourceKind {
    /// An ORC certificate, copied from the catalogue.
    Orc {
        /// The record as the catalogue held it.
        record: Box<OrcRecord>,
    },
    /// An imported Expedition or Adrena polar, parsed at import.
    PolarFile {
        /// How it was read.
        format: PolarFileFormat,
        /// The file name it came from.
        file_name: String,
        /// The grid as parsed.
        polar: PolarGrid,
    },
    /// A track.
    Track {
        /// The track; its bulk data lives in `tracks/<id>.json`.
        track: Box<Track>,
    },
}

impl SourceKind {
    /// A stable name for the kind, for IPC and labels.
    pub fn name(&self) -> &'static str {
        match self {
            Self::Orc { .. } => "orc",
            Self::PolarFile { .. } => "polar_file",
            Self::Track { .. } => "track",
        }
    }
}

/// Every user change stored beside one source (spec.md 4.1).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Overlay {
    /// Polar edits (spec.md 10.4), applied before blending.
    pub cell_overrides: Vec<CellOverride>,
    /// Polar nodes excluded from the blend (spec.md 10.3).
    pub excluded_cells: Vec<CellRef>,
    /// Track dots removed from the blend (spec.md 10.3), sorted.
    pub excluded_samples: Vec<SampleId>,
    /// Sample filters; track sources only (spec.md 7.6).
    pub filters: SampleFilters,
}

impl Overlay {
    /// Whether the overlay changes nothing.
    pub fn is_empty(&self) -> bool {
        self.cell_overrides.is_empty()
            && self.excluded_cells.is_empty()
            && self.excluded_samples.is_empty()
            && self.filters == SampleFilters::default()
    }
}

/// One edited cell.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellOverride {
    /// TWA, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub twa: f64,
    /// TWS, knots.
    #[serde(with = "canonical::knots_field")]
    pub tws: f64,
    /// The boat speed the user gave it, knots.
    #[serde(with = "canonical::knots_field")]
    pub bsp: f64,
}

/// A cell named by its axis values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CellRef {
    /// TWA, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub twa: f64,
    /// TWS, knots.
    #[serde(with = "canonical::knots_field")]
    pub tws: f64,
}

/// A closed range; either end may be open.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Range {
    /// Lower bound, if any.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub min: Option<f64>,
    /// Upper bound, if any.
    #[serde(default, with = "canonical::optional_ratio_field")]
    pub max: Option<f64>,
}

/// A range of directions in degrees, clockwise from `from` to `to`; it may
/// wrap through north (`from` 300, `to` 60).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DirectionRange {
    /// Start, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub from: f64,
    /// End, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub to: f64,
}

/// Where waves come from relative to the bow (spec.md 7.6).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WaveSector {
    /// Head seas.
    Head,
    /// On the bow.
    Bow,
    /// On the beam.
    Beam,
    /// On the quarter.
    Quarter,
    /// Following seas.
    Following,
}

/// The wave-direction filter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum WaveDirectionFilter {
    /// Keep samples whose waves come from one of these sectors.
    Sectors {
        /// The sectors kept.
        sectors: Vec<WaveSector>,
    },
    /// Keep samples whose wave angle off the bow is in this range, degrees
    /// in [0, 180].
    Relative {
        /// The range kept.
        range: Range,
    },
    /// Keep samples whose waves come from this compass range.
    Absolute {
        /// The "from" directions kept.
        range: DirectionRange,
    },
}

/// Which heading and speed values pass the filter (spec.md 7.6).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OriginFilter {
    /// Both.
    #[default]
    Any,
    /// Only values the track supplied.
    GivenOnly,
    /// Only values derived from neighbours.
    DerivedOnly,
}

/// A track source's sample filters (spec.md 7.6). Every bound is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SampleFilters {
    /// Significant wave height, metres.
    pub wave_height_m: Option<Range>,
    /// Wave direction.
    pub wave_direction: Option<WaveDirectionFilter>,
    /// Current speed, knots.
    pub current_speed_kn: Option<Range>,
    /// True wind speed, knots.
    pub tws_kn: Option<Range>,
    /// True wind angle, degrees.
    pub twa_deg: Option<Range>,
    /// Time window, UTC epoch seconds. Import sets it to the race start and
    /// finish when the tracker gives them.
    pub time_window: Option<TimeWindow>,
    /// Minimum boat speed, knots.
    #[serde(with = "canonical::optional_knots_field")]
    pub min_bsp_kn: Option<f64>,
    /// Maximum boat speed, knots.
    #[serde(with = "canonical::optional_knots_field")]
    pub max_bsp_kn: Option<f64>,
    /// Exclude fixes whose heading changes by more than this between
    /// neighbours (manoeuvres), degrees.
    #[serde(with = "canonical::optional_degrees_field")]
    pub max_heading_change_deg: Option<f64>,
    /// Given versus derived heading.
    pub heading_origin: OriginFilter,
    /// Given versus derived speed.
    pub speed_origin: OriginFilter,
    /// Exclude samples whose current came from a tier without tides
    /// (spec.md 7.5.1).
    pub exclude_no_tide: bool,
}

impl Default for SampleFilters {
    fn default() -> Self {
        Self {
            wave_height_m: None,
            wave_direction: None,
            current_speed_kn: None,
            tws_kn: None,
            twa_deg: None,
            time_window: None,
            min_bsp_kn: Some(1.0),
            max_bsp_kn: None,
            max_heading_change_deg: Some(30.0),
            heading_origin: OriginFilter::Any,
            speed_origin: OriginFilter::Any,
            exclude_no_tide: false,
        }
    }
}

/// A time window, UTC epoch seconds; either end may be open.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Start.
    #[serde(default)]
    pub start: Option<i64>,
    /// End.
    #[serde(default)]
    pub end: Option<i64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn colours_are_checked_and_lower_cased() {
        assert_eq!(Colour::parse("#A1B2C3").unwrap().as_str(), "#a1b2c3");
        for bad in ["a1b2c3", "#abc", "#gggggg", "#a1b2c3d", ""] {
            assert!(Colour::parse(bad).is_err(), "{bad}");
        }
        assert!(serde_json::from_str::<Colour>("\"red\"").is_err());
        for colour in PALETTE.into_iter().chain(["#ffffff"]) {
            assert_eq!(Colour::parse(colour).unwrap(), Colour::trusted(colour));
        }
    }

    #[test]
    fn the_default_filters_match_the_spec() {
        let filters = SampleFilters::default();
        assert_eq!(filters.min_bsp_kn, Some(1.0));
        assert_eq!(filters.max_heading_change_deg, Some(30.0));
        assert!(Overlay::default().is_empty());
    }

    #[test]
    fn weights_outside_zero_to_two_are_refused() {
        validate_weight(0.0).unwrap();
        validate_weight(2.0).unwrap();
        assert!(validate_weight(2.01).is_err());
        assert!(validate_weight(-0.1).is_err());
        assert!(validate_weight(f64::NAN).is_err());
    }
}
