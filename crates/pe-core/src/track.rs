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
    /// Whether the environment fetch has answered for this sample (even if
    /// it found nothing there, as over land for waves or before an
    /// archive's first hour). What a Refetch resumes from: samples without
    /// it are the ones still to fetch (spec.md 7.7).
    #[serde(default)]
    pub env_fetched: bool,
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
            env_fetched: false,
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

    /// Forgets everything the environment fetch stored for this sample —
    /// wind, waves and current, raw and corrected, and which datasets
    /// supplied them — as a Refetch that starts over does, so no value of
    /// an earlier fetch survives next to the new one.
    pub fn clear_env(&mut self) {
        self.tws = None;
        self.twd_from = None;
        self.hs_m = None;
        self.wave_from = None;
        self.current_speed = None;
        self.current_toward = None;
        self.wind_dataset = None;
        self.wave_dataset = None;
        self.current_dataset = None;
        self.env_fetched = false;
        self.relate();
    }

    /// Recomputes everything that relates the boat's motion to the stored
    /// environment (spec.md 7.5, D13): TWA and tack over the ground, the
    /// current-corrected motion and wind, and the wave angle off the bow.
    ///
    /// Reads only the stored motion and environment, so a change of
    /// derivation settings recomputes these without fetching anything
    /// (M8 carry), and the environment fetch calls it once it has stored
    /// what it found. Plain vector arithmetic on the stored values, which is
    /// why it lives with the data rather than in `pe-tracks` or `pe-env`.
    ///
    /// - Wind over the ground: TWA is the angle between the heading and
    ///   where the wind comes from, 0–180°; the wind on the starboard side
    ///   is starboard tack.
    /// - With a current: the boat's velocity through the water is its
    ///   ground velocity minus the current (leeway ignored), and the wind
    ///   over the water is the wind minus the current. Without one, every
    ///   corrected value is empty.
    /// - The wave angle is measured off the bow, which points along the
    ///   heading through the water where there is a current and the ground
    ///   heading otherwise: 0° head seas, 180° following.
    pub fn relate(&mut self) {
        self.twa = self
            .heading
            .zip(self.twd_from)
            .map(|(h, w)| angle_off(h, w));
        self.tack = self
            .heading
            .zip(self.twd_from)
            .and_then(|(h, w)| tack_of(h, w));
        self.bsp_corrected = None;
        self.heading_corrected = None;
        self.tws_corrected = None;
        self.twd_from_corrected = None;
        self.twa_corrected = None;
        self.tack_corrected = None;
        if let (Some(cs), Some(ct)) = (self.current_speed, self.current_toward) {
            let (ce, cn) = toward(cs, ct);
            if let (Some(h), Some(s)) = (self.heading, self.speed) {
                let (ge, gn) = toward(s, h);
                let (we, wn) = (ge - ce, gn - cn);
                let bsp = we.hypot(wn);
                self.bsp_corrected = Some(bsp);
                // A boat stopped in the water has no heading through it.
                self.heading_corrected = (bsp > 1e-9).then(|| compass(we, wn));
            }
            if let (Some(tws), Some(from)) = (self.tws, self.twd_from) {
                let (ae, an) = toward(tws, from + 180.0);
                let (re, rn) = (ae - ce, an - cn);
                let speed = re.hypot(rn);
                self.tws_corrected = Some(speed);
                self.twd_from_corrected =
                    (speed > 1e-9).then(|| (compass(re, rn) + 180.0).rem_euclid(360.0));
            }
            let pair = self.heading_corrected.zip(self.twd_from_corrected);
            self.twa_corrected = pair.map(|(h, w)| angle_off(h, w));
            self.tack_corrected = pair.and_then(|(h, w)| tack_of(h, w));
        }
        let bow = self.heading_corrected.or(self.heading);
        self.wave_angle = bow.zip(self.wave_from).map(|(h, w)| angle_off(h, w));
    }
}

/// East and north components of `speed` toward compass `direction`.
fn toward(speed: f64, direction: f64) -> (f64, f64) {
    let r = direction.to_radians();
    (speed * r.sin(), speed * r.cos())
}

/// The compass direction a vector points toward, degrees in [0, 360).
pub fn compass(east: f64, north: f64) -> f64 {
    let d = east.atan2(north).to_degrees().rem_euclid(360.0);
    // rem_euclid can round a tiny negative up to exactly 360.
    if d >= 360.0 { 0.0 } else { d }
}

/// The unsigned angle between two compass directions, [0, 180].
fn angle_off(a: f64, b: f64) -> f64 {
    let d = (a - b).rem_euclid(360.0);
    if d > 180.0 { 360.0 - d } else { d }
}

/// The tack for a heading and a wind "from": wind over the starboard side
/// (coming from 0–180° clockwise of the bow) is starboard tack. Head to
/// wind or dead downwind is neither.
fn tack_of(heading: f64, from: f64) -> Option<Tack> {
    let r = (from - heading).rem_euclid(360.0);
    if r > 0.0 && r < 180.0 {
        Some(Tack::Starboard)
    } else if r > 180.0 && r < 360.0 {
        Some(Tack::Port)
    } else {
        None
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
    /// The wind and wave sampling interval of the last fetch, seconds (3600
    /// hourly, 10800 3-hourly, D19); `None` before any fetch. A Refetch
    /// at another interval starts over rather than mixing the two.
    #[serde(default)]
    pub interval_s: Option<i64>,
    /// Whether the last fetch added Stokes drift to the global merged
    /// current (spec.md 7.5.1); `None` before any fetch. Like the interval,
    /// a Refetch with the other choice starts over, so a track never mixes
    /// currents with and without it.
    #[serde(default)]
    pub stokes_drift: Option<bool>,
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
    /// that are not currents; every current tier has tide today, GlobCurrent
    /// included (`Some(true)`, FES2022, spec.md 7.5.1). `Some(false)` is
    /// kept for a future tier without one, which the "leave out currents
    /// without tide" filter can still remove.
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

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Sample {
        let fix = Fix {
            t: 0,
            lat: 50.0,
            lon: -5.0,
            cog: None,
            sog: None,
        };
        Sample::at(SampleId(1), 0, &fix)
    }

    fn close(a: Option<f64>, b: f64) -> bool {
        a.is_some_and(|a| (a - b).abs() < 1e-9)
    }

    /// No current: TWA off the ground heading, tack by the side the wind is
    /// on, nothing corrected; waves from astern are following seas.
    #[test]
    fn without_a_current_only_ground_values_are_related() {
        let mut s = sample();
        s.heading = Some(0.0);
        s.speed = Some(6.0);
        s.tws = Some(10.0);
        s.twd_from = Some(45.0);
        s.wave_from = Some(180.0);
        s.relate();
        assert!(close(s.twa, 45.0));
        assert_eq!(s.tack, Some(Tack::Starboard));
        assert_eq!(s.bsp_corrected, None);
        assert_eq!(s.twa_corrected, None);
        assert!(close(s.wave_angle, 180.0));
        s.heading = Some(90.0);
        s.twd_from = Some(0.0);
        s.relate();
        assert!(close(s.twa, 90.0));
        assert_eq!(s.tack, Some(Tack::Port));
    }

    /// Hand-computed (D13): heading 0° at 6 kn in 1 kn of current toward
    /// 090°. Through the water (-1, 6): BSP √37, heading 360 − atan(1/6) =
    /// 350.537677792°. Wind 10 kn from 0°, i.e. (0, −10) toward, minus the
    /// current: (−1, −10), √101 kn from atan(1/10) = 5.710593137°. TWA
    /// through the water 5.710593137 + 9.462322208 = 15.172915345°,
    /// starboard. Waves from 350.537677792° are dead ahead of the bow.
    #[test]
    fn a_current_is_taken_out_of_the_motion_and_the_wind() {
        let mut s = sample();
        s.heading = Some(0.0);
        s.speed = Some(6.0);
        s.tws = Some(10.0);
        s.twd_from = Some(0.0);
        s.current_speed = Some(1.0);
        s.current_toward = Some(90.0);
        s.wave_from = Some(350.537_677_792);
        s.relate();
        assert!(close(s.bsp_corrected, 37f64.sqrt()));
        assert!(close(s.heading_corrected, 350.537_677_791_974_9));
        assert!(close(s.tws_corrected, 101f64.sqrt()));
        assert!(close(s.twd_from_corrected, 5.710_593_137_499_643));
        assert!(close(s.twa_corrected, 15.172_915_345_524_7));
        assert_eq!(s.tack_corrected, Some(Tack::Starboard));
        assert!(s.wave_angle.is_some_and(|a| a < 1e-6), "{:?}", s.wave_angle);
        // Ground values are untouched by the current.
        assert!(close(s.twa, 0.0));
        assert_eq!(s.tack, None, "head to wind is neither tack");
    }

    /// Clearing leaves nothing of the fetch: no raw, corrected or
    /// provenance value, and not fetched.
    #[test]
    fn clearing_the_environment_leaves_only_the_motion() {
        let mut s = sample();
        s.heading = Some(0.0);
        s.speed = Some(6.0);
        s.tws = Some(10.0);
        s.twd_from = Some(0.0);
        s.hs_m = Some(1.0);
        s.wave_from = Some(10.0);
        s.current_speed = Some(1.0);
        s.current_toward = Some(90.0);
        s.wind_dataset = Some(0);
        s.wave_dataset = Some(1);
        s.current_dataset = Some(2);
        s.env_fetched = true;
        s.relate();
        s.clear_env();
        let mut bare = sample();
        bare.heading = Some(0.0);
        bare.speed = Some(6.0);
        assert_eq!(s, bare);
    }

    /// Without a heading there is no angle, and a missing speed leaves the
    /// water track unknown while the wind over the water is still known.
    #[test]
    fn missing_motion_leaves_angles_empty() {
        let mut s = sample();
        s.tws = Some(10.0);
        s.twd_from = Some(0.0);
        s.current_speed = Some(1.0);
        s.current_toward = Some(90.0);
        s.wave_from = Some(0.0);
        s.relate();
        assert_eq!((s.twa, s.tack, s.wave_angle), (None, None, None));
        assert_eq!(s.bsp_corrected, None);
        assert!(close(s.tws_corrected, 101f64.sqrt()));
        assert_eq!(s.twa_corrected, None);
    }
}
