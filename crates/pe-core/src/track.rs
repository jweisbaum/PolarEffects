//! A track as plain data: where it came from, its fixes as imported, and the
//! samples derived from them with the environment found for each.
//!
//! Filled by `pe-tracks` (import, derivation, filters) and `pe-env`
//! (reanalysis). The fields are those of spec.md 7.4 and 7.5 in full, so the
//! later milestones fill them in without a schema migration.
//!
//! **Bulk data lives outside `project.json`.** `fixes` and `samples` are
//! skipped by serde here and written to `tracks/<id>.json` by [`crate::io`],
//! so the project document stays small and diffable (spec.md 4.3).

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::id::{SampleId, TrackId};

/// One imported track.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Track {
    /// Names the archive entry holding this track's fixes and samples.
    pub id: TrackId,
    /// Where it came from.
    pub origin: TrackOrigin,
    /// How heading and speed are derived (spec.md 7.4).
    #[serde(default)]
    pub derivation: DerivationSettings,
    /// How this track's polar segment summarises a cell (spec.md 12.1).
    #[serde(default)]
    pub statistic: SegmentStatistic,
    /// Which reanalysis datasets supplied the samples.
    #[serde(default)]
    pub env_meta: EnvMeta,
    /// The fixes exactly as imported, sorted by time with duplicates merged
    /// (spec.md 7.4). Never rewritten afterwards (invariant 1).
    #[serde(skip)]
    pub fixes: Vec<Fix>,
    /// One sample per fix, derived and fetched.
    #[serde(skip)]
    pub samples: Vec<Sample>,
}

impl Track {
    /// A track with no fixes yet.
    pub fn new(id: TrackId, origin: TrackOrigin) -> Self {
        Self {
            id,
            origin,
            derivation: DerivationSettings::default(),
            statistic: SegmentStatistic::default(),
            env_meta: EnvMeta::default(),
            fixes: Vec::new(),
            samples: Vec::new(),
        }
    }

    /// The bulk half, as written to `tracks/<id>.json`.
    pub fn bulk(&self) -> TrackBulkRef<'_> {
        TrackBulkRef {
            fixes: &self.fixes,
            samples: &self.samples,
        }
    }
}

/// The part of a track written to its own archive entry.
#[derive(Debug, Serialize)]
pub struct TrackBulkRef<'a> {
    /// See [`Track::fixes`].
    pub fixes: &'a [Fix],
    /// See [`Track::samples`].
    pub samples: &'a [Sample],
}

/// [`TrackBulkRef`] as read back.
#[derive(Debug, Default, Deserialize)]
pub struct TrackBulk {
    /// See [`Track::fixes`].
    #[serde(default)]
    pub fixes: Vec<Fix>,
    /// See [`Track::samples`].
    #[serde(default)]
    pub samples: Vec<Sample>,
}

/// Which tracker a track was scraped from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tracker {
    /// YellowBrick (`yb.tl`).
    YellowBrick,
    /// Geovoile (`*.geovoile.com`).
    Geovoile,
    /// Blue Water Tracks.
    BlueWaterTracks,
}

/// Where a track came from (spec.md 4.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum TrackOrigin {
    /// One boat of a tracker event.
    Tracker {
        /// Which tracker.
        tracker: Tracker,
        /// The event URL the user pasted.
        event_url: String,
        /// The event's title as the tracker gives it.
        event_title: String,
        /// The tracker's own id for the boat.
        boat_id: String,
        /// Boat name.
        boat_name: String,
        /// Sail number, if the tracker gives one.
        #[serde(default)]
        sail_no: Option<String>,
        /// The race start as the tracker gives it, UTC epoch seconds. The
        /// default start of the time-window filter (spec.md 7.6).
        #[serde(default)]
        race_start: Option<i64>,
        /// The race finish as the tracker gives it, UTC epoch seconds.
        #[serde(default)]
        race_finish: Option<i64>,
    },
    /// A GeoJSON or CSV file.
    File {
        /// The file name it was imported from.
        name: String,
        /// The boat name, when the file has several boats.
        #[serde(default)]
        boat_name: Option<String>,
    },
}

/// One timestamped position, as imported.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    /// UTC epoch seconds.
    pub t: i64,
    /// Latitude, degrees.
    #[serde(with = "canonical::degrees_field")]
    pub lat: f64,
    /// Longitude, degrees in [-180, 180).
    #[serde(with = "canonical::degrees_field")]
    pub lon: f64,
    /// Course over ground the track supplied, degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub cog: Option<f64>,
    /// Speed over ground (or boat speed) the track supplied, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub sog: Option<f64>,
}

/// Whether a value came with the track or was computed from neighbours.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ValueOrigin {
    /// Supplied by the tracker or file.
    Given,
    /// Derived from neighbouring fixes (spec.md 7.4).
    Derived,
}

/// Which side the wind is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Tack {
    /// Wind over the port side.
    Port,
    /// Wind over the starboard side.
    Starboard,
}

/// One track position with its derived motion and the environment found for
/// it (spec.md 2, 7.4, 7.5). A dot in the plots.
///
/// **Raw** values are ground-relative, as the tracker and the reanalysis give
/// them. **Corrected** values are water-relative, with the current taken out
/// (spec.md 7.5, D13); they are present only where a current was found. A
/// project setting chooses which feed the polar. Everything the pipeline may
/// fail to find is optional.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Sample {
    /// Named by exclusions in the overlay.
    pub id: SampleId,
    /// Index of the fix this sample is for.
    pub fix: u32,
    /// UTC epoch seconds (the fix's).
    pub t: i64,
    /// Latitude, degrees (the fix's).
    #[serde(with = "canonical::degrees_field")]
    pub lat: f64,
    /// Longitude, degrees (the fix's).
    #[serde(with = "canonical::degrees_field")]
    pub lon: f64,

    // --- Motion over the ground (spec.md 7.4) ---
    /// Heading over the ground, degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub heading: Option<f64>,
    /// Whether [`Self::heading`] was given or derived.
    #[serde(default)]
    pub heading_origin: Option<ValueOrigin>,
    /// Speed over the ground, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub speed: Option<f64>,
    /// Whether [`Self::speed`] was given or derived.
    #[serde(default)]
    pub speed_origin: Option<ValueOrigin>,

    // --- Wind, raw (ground-relative) ---
    /// True wind speed over the ground, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub tws: Option<f64>,
    /// True wind direction, meteorological "from", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twd_from: Option<f64>,
    /// True wind angle off the ground heading, degrees in [0, 180].
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twa: Option<f64>,
    /// Tack from the ground heading and wind.
    #[serde(default)]
    pub tack: Option<Tack>,

    // --- Water-relative, current removed (spec.md 7.5, D13) ---
    /// Boat speed through the water, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub bsp_corrected: Option<f64>,
    /// Heading through the water, degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub heading_corrected: Option<f64>,
    /// True wind speed over the water, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub tws_corrected: Option<f64>,
    /// True wind direction over the water, "from", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twd_from_corrected: Option<f64>,
    /// True wind angle through the water, degrees in [0, 180].
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub twa_corrected: Option<f64>,
    /// Tack through the water.
    #[serde(default)]
    pub tack_corrected: Option<Tack>,

    // --- Waves ---
    /// Significant wave height, metres.
    #[serde(default, with = "canonical::optional_metres_field")]
    pub hs_m: Option<f64>,
    /// Mean wave direction, "from", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub wave_from: Option<f64>,
    /// Angle between the bow and where the waves come from, degrees in
    /// [0, 180]: 0 head seas, 180 following.
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub wave_angle: Option<f64>,

    // --- Current ---
    /// Surface current speed, knots.
    #[serde(default, with = "canonical::optional_knots_field")]
    pub current_speed: Option<f64>,
    /// Surface current direction, oceanographic "toward", degrees in [0, 360).
    #[serde(default, with = "canonical::optional_degrees_field")]
    pub current_toward: Option<f64>,

    // --- Provenance: indices into the track's `env_meta.datasets` ---
    /// Which dataset supplied the wind.
    #[serde(default)]
    pub wind_dataset: Option<u16>,
    /// Which dataset supplied the waves.
    #[serde(default)]
    pub wave_dataset: Option<u16>,
    /// Which dataset supplied the current.
    #[serde(default)]
    pub current_dataset: Option<u16>,
}

impl Sample {
    /// A sample at a fix with nothing derived or fetched yet.
    pub fn at(id: SampleId, fix_index: u32, fix: &Fix) -> Self {
        Self {
            id,
            fix: fix_index,
            t: fix.t,
            lat: fix.lat,
            lon: fix.lon,
            heading: None,
            heading_origin: None,
            speed: None,
            speed_origin: None,
            tws: None,
            twd_from: None,
            twa: None,
            tack: None,
            bsp_corrected: None,
            heading_corrected: None,
            tws_corrected: None,
            twd_from_corrected: None,
            twa_corrected: None,
            tack_corrected: None,
            hs_m: None,
            wave_from: None,
            wave_angle: None,
            current_speed: None,
            current_toward: None,
            wind_dataset: None,
            wave_dataset: None,
            current_dataset: None,
        }
    }
}

/// A sample's motion over the ground: heading and speed with where each
/// came from (spec.md 7.4). What re-deriving a track rewrites, and all it
/// rewrites; [`crate::Command::SetDerivation`] carries it both ways.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct Motion {
    /// Heading over the ground, degrees in [0, 360).
    pub heading: Option<f64>,
    /// Whether the heading was given or derived.
    pub heading_origin: Option<ValueOrigin>,
    /// Speed over the ground, knots.
    pub speed: Option<f64>,
    /// Whether the speed was given or derived.
    pub speed_origin: Option<ValueOrigin>,
}

impl Sample {
    /// This sample's motion.
    pub fn motion(&self) -> Motion {
        Motion {
            heading: self.heading,
            heading_origin: self.heading_origin,
            speed: self.speed,
            speed_origin: self.speed_origin,
        }
    }

    /// Replaces this sample's motion.
    pub fn set_motion(&mut self, motion: Motion) {
        self.heading = motion.heading;
        self.heading_origin = motion.heading_origin;
        self.speed = motion.speed;
        self.speed_origin = motion.speed_origin;
    }
}

/// Which value to use when a fix has both a given and a derived one.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PreferValues {
    /// The tracker's or file's own heading and speed, where present.
    #[default]
    Given,
    /// Always the derived ones.
    Derived,
}

/// How heading and speed are derived for a track (spec.md 7.4).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct DerivationSettings {
    /// Longest gap between neighbours a central difference may span, seconds.
    /// Beyond it the fix gets no derived values.
    pub max_gap_s: i64,
    /// Given or derived values first.
    pub prefer: PreferValues,
}

/// The longest maximum gap offered, seconds: a central difference across
/// more than a day says nothing about how the boat was sailing.
pub const MAX_GAP_LIMIT_S: i64 = 24 * 3600;

impl DerivationSettings {
    /// Checks the settings are ones the derivation can use.
    pub fn validate(&self) -> crate::Result<()> {
        if (1..=MAX_GAP_LIMIT_S).contains(&self.max_gap_s) {
            Ok(())
        } else {
            Err(crate::CoreError::Invalid(format!(
                "a maximum gap of {} s is outside 1 s to {MAX_GAP_LIMIT_S} s",
                self.max_gap_s
            )))
        }
    }
}

impl Default for DerivationSettings {
    fn default() -> Self {
        Self {
            max_gap_s: 3 * 3600,
            prefer: PreferValues::Given,
        }
    }
}

/// The per-cell statistic of a track's polar segment (spec.md 12.1, D18).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SegmentStatistic {
    /// Median.
    Median,
    /// Mean.
    Mean,
    /// 75th percentile.
    P75,
    /// 90th percentile: a polar describes good sailing, not average sailing.
    #[default]
    P90,
}

/// Which reanalysis a track's samples came from (spec.md 4.1).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct EnvMeta {
    /// Every dataset any sample refers to, by index.
    pub datasets: Vec<DatasetRecord>,
    /// How far the environment fetch got.
    pub status: EnvStatus,
}

/// One dataset a track's environment was taken from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DatasetRecord {
    /// Dataset name, e.g. `"weatherbench2-era5"`.
    pub name: String,
    /// Dataset version as recorded by the archive.
    pub version: String,
    /// When it was fetched, UTC epoch seconds.
    pub fetched_at: i64,
    /// Whether this dataset's current includes tides. `None` for datasets
    /// that are not currents; `Some(false)` marks the GlobCurrent tier
    /// (spec.md 7.5.1), which can be filtered out.
    #[serde(default)]
    pub has_tide: Option<bool>,
}

/// How far a track's environment fetch got (spec.md 7.1). "Fetching" is not
/// stored: a job is not project data.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnvStatus {
    /// Nothing fetched yet.
    #[default]
    NotFetched,
    /// Every sample has its environment.
    Ready,
    /// Some samples have it; a Refetch resumes.
    Partial,
    /// The fetch failed with nothing to show.
    Failed,
}
