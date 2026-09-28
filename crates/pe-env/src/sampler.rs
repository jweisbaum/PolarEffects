//! The environment at track positions (spec.md 7.5, 7.5.1).
//!
//! A [`Provider`] answers, for a batch of positions, the wind, waves and
//! current there. [`Reanalysis`] is the real one: it reads WeatherBench2
//! and ARCO-ERA5 for wind and waves and walks the current tiers. The job
//! system in `pe-app` talks to the trait, so its cancel-and-resume
//! behaviour is tested against a fake without the network.
//!
//! **Each position's answer depends only on that position** (its time,
//! latitude and longitude) and the options: never on which other positions
//! share its batch. That is what lets a cancelled fetch resume to exactly
//! the result an uninterrupted one gives.
//!
//! Interpolation (spec.md 7.5): bilinear in space, linear in time. Wind and
//! current interpolate their u and v; wave direction interpolates as a unit
//! vector. Land (NaN) corners are left out of the stencil; if all are land
//! the value is missing. Values stay in archive units (m/s, m, degrees);
//! `pe-app` converts to knots once, on ingest into the project.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use crate::cache::ChunkCache;
use crate::dataset::{
    Cell, Dataset, OpenVariable, Variable, corner_cells, corners_of, lerp_time, vars,
};
use crate::error::Result;
use crate::grid::Stencil;
use crate::http::Interrupt;
use crate::store::{OpenStore, TimeAxis, open_dir, open_http_interruptible};

/// One track position to sample.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    /// UTC epoch seconds.
    pub t: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees, any convention.
    pub lon: f64,
}

/// How finely wind and waves are sampled in time (D19). Currents come from
/// the geoChunked stores, where every hour costs the same, and are always
/// hourly.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord)]
pub enum Interval {
    /// Every hour of ERA5 (the default).
    #[default]
    Hourly,
    /// Every third hour (00, 03, … UTC), interpolated between: a third of
    /// the download, for long races.
    ThreeHourly,
}

impl Interval {
    /// The spacing, seconds.
    pub fn seconds(self) -> i64 {
        match self {
            Self::Hourly => 3600,
            Self::ThreeHourly => 3 * 3600,
        }
    }

    /// The interval of this spacing, if it is one.
    pub fn from_seconds(seconds: i64) -> Option<Self> {
        [Self::Hourly, Self::ThreeHourly]
            .into_iter()
            .find(|i| i.seconds() == seconds)
    }
}

/// What a fetch reads.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Options {
    /// Wind and wave time sampling.
    pub interval: Interval,
    /// Add Stokes drift to the global merged current (spec.md 7.5.1; off by
    /// default, Q3).
    pub stokes_drift: bool,
}

/// A horizontal vector, m/s: `u` east, `v` north. Wind "from" and current
/// "toward" are worked out by the caller (CLAUDE.md conventions).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Vector {
    /// Eastward, m/s.
    pub u: f64,
    /// Northward, m/s.
    pub v: f64,
    /// Where it came from.
    pub dataset: Dataset,
}

/// Waves at a position; height and direction may each be missing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Waves {
    /// Significant wave height, metres.
    pub hs: Option<f64>,
    /// Mean wave direction, "from", degrees in [0, 360).
    pub from: Option<f64>,
    /// Where they came from.
    pub dataset: Dataset,
}

/// Everything found at one position.
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct EnvPoint {
    /// 10 m wind.
    pub wind: Option<Vector>,
    /// Waves.
    pub waves: Option<Waves>,
    /// Surface current (toward).
    pub current: Option<Vector>,
}

/// Anything that can say what the environment was at track positions.
pub trait Provider: Send + Sync {
    /// The environment at each of `points`, in order.
    ///
    /// # Errors
    /// [`crate::EnvError::Cancelled`] once `cancel` is set; a read error
    /// otherwise. Nothing partial is returned: a batch is all or nothing.
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<EnvPoint>>;

    /// What the last calls had to leave out and why (a current tier whose
    /// store would not open), each said once; emptied by the call.
    fn take_warnings(&self) -> Vec<String> {
        Vec::new()
    }
}

/// How long a current tier whose store would not open is left out before
/// it is tried again.
const TIER_RETRY: Duration = Duration::from_secs(600);

/// How [`Reanalysis`] reaches the archives.
#[derive(Debug, Clone)]
pub enum Access {
    /// Over HTTPS, through the chunk cache (the app).
    Http {
        /// Per-request timeout (spec.md 3.4).
        timeout: Duration,
        /// The shared chunk cache: every track of every event reads
        /// through it, so the second boat of a race costs almost nothing.
        cache: Arc<ChunkCache>,
    },
    /// From recorded stores in folders (tests). A dataset without a folder
    /// is unavailable, as if it had no data anywhere.
    Dirs(BTreeMap<Dataset, PathBuf>),
}

/// The real provider: the archives of spec.md 7.5 and 7.5.1.
///
/// Opened stores and arrays are kept for the life of the provider: each
/// open costs about a second of metadata requests (M3), which a job over
/// several tracks would otherwise pay per track.
pub struct Reanalysis {
    access: Access,
    concurrency: usize,
    stores: Mutex<BTreeMap<Dataset, OpenStore>>,
    opened: Mutex<BTreeMap<(Dataset, &'static str), Arc<OpenVariable>>>,
    /// The running job's cancel flag, watched by every store's retries.
    interrupt: Arc<Interrupt>,
    /// Current tiers whose store would not open, and when.
    unavailable: Mutex<BTreeMap<Dataset, std::time::Instant>>,
    warnings: Mutex<Vec<String>>,
}

impl std::fmt::Debug for Reanalysis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reanalysis")
            .field("access", &self.access)
            .field("concurrency", &self.concurrency)
            .finish_non_exhaustive()
    }
}

/// The GlobCurrent, NW Shelf and IBI boxes, a little wider than their
/// grids: a position outside is never looked for there, so a race
/// elsewhere never opens those stores.
const NWS_BOX: ([f64; 2], [f64; 2]) = ([40.0, 65.1], [-20.0, 13.1]);
const IBI_BOX: ([f64; 2], [f64; 2]) = ([26.1, 56.1], [-19.1, 5.1]);

fn in_box(p: &Point, (lat, lon): ([f64; 2], [f64; 2])) -> bool {
    let x = (p.lon + 180.0).rem_euclid(360.0) - 180.0;
    (lat[0]..=lat[1]).contains(&p.lat) && (lon[0]..=lon[1]).contains(&x)
}

/// Positions placed on a variable, and the corner values they need.
type Placed = (Vec<Option<Stamp>>, BTreeMap<Cell, f32>);

/// A vector at each position, `None` where any part is missing.
type Vectors = Vec<Option<(f64, f64)>>;

/// A current tier: its dataset, the (u, v) pairs summed, and the box it is
/// looked for in (`None`: everywhere).
type Tier = (
    Dataset,
    Vec<(Variable, Variable)>,
    Option<([f64; 2], [f64; 2])>,
);

/// A position placed on a variable's grid and time axis.
#[derive(Debug, Clone, Copy)]
struct Stamp {
    stencil: Stencil,
    a: u64,
    b: u64,
    w: f64,
}

/// The steps either side of `t` on `axis`, taking only every `every`
/// seconds (aligned on UTC midnight), and the weight of the later one.
/// Falls back to every step where the coarse steps are not on the axis
/// (its end, or an axis whose spacing does not divide `every`).
fn bracket_every(axis: &TimeAxis, t: i64, every: i64) -> Option<(u64, u64, f64)> {
    if every <= axis.step || every % axis.step != 0 {
        return axis.bracket(t);
    }
    let ta = t - t.rem_euclid(every);
    if ta == t {
        return axis.bracket(t);
    }
    let coarse = match (axis.bracket(ta), axis.bracket(ta + every)) {
        (Some((a, a2, _)), Some((b, b2, _))) if a == a2 && b == b2 => {
            Some((a, b, (t - ta) as f64 / every as f64))
        }
        _ => None,
    };
    coarse.or_else(|| axis.bracket(t))
}

/// Interpolates one side's corners, NaN corners left out.
fn side(stencil: &Stencil, corners: [f32; 4]) -> Option<f64> {
    stencil.interpolate(corners)
}

impl Reanalysis {
    /// A provider reading the archives over HTTPS through `cache`, with at
    /// most `concurrency` chunk reads at once (spec.md 3.4).
    pub fn http(timeout: Duration, cache: Arc<ChunkCache>, concurrency: usize) -> Self {
        Self::new(Access::Http { timeout, cache }, concurrency)
    }

    /// A provider over recorded stores.
    pub fn new(access: Access, concurrency: usize) -> Self {
        Self {
            access,
            concurrency: concurrency.max(1),
            stores: Mutex::new(BTreeMap::new()),
            opened: Mutex::new(BTreeMap::new()),
            interrupt: Arc::new(Interrupt::default()),
            unavailable: Mutex::new(BTreeMap::new()),
            warnings: Mutex::new(Vec::new()),
        }
    }

    /// The chunk cache it reads through, when it reads over HTTPS.
    pub fn cache(&self) -> Option<&Arc<ChunkCache>> {
        match &self.access {
            Access::Http { cache, .. } => Some(cache),
            Access::Dirs(_) => None,
        }
    }

    fn store(&self, dataset: Dataset) -> Result<Option<OpenStore>> {
        let mut stores = self
            .stores
            .lock()
            .map_err(|_| crate::EnvError::Open("the store table lock was poisoned".into()))?;
        if let Some(store) = stores.get(&dataset) {
            return Ok(Some(store.clone()));
        }
        let store = match &self.access {
            Access::Http { timeout, cache } => open_http_interruptible(
                dataset.url(),
                *timeout,
                Some((Arc::clone(cache), dataset.id())),
                Arc::clone(&self.interrupt),
            )?,
            Access::Dirs(dirs) => match dirs.get(&dataset) {
                Some(dir) => open_dir(dir)?,
                None => return Ok(None),
            },
        };
        stores.insert(dataset, store.clone());
        Ok(Some(store))
    }

    /// The opened variable, or `None` when its dataset is unavailable.
    fn var(&self, spec: Variable) -> Result<Option<Arc<OpenVariable>>> {
        let key = (spec.dataset, spec.array);
        if let Some(open) = self
            .opened
            .lock()
            .ok()
            .and_then(|opened| opened.get(&key).cloned())
        {
            return Ok(Some(open));
        }
        let Some(store) = self.store(spec.dataset)? else {
            return Ok(None);
        };
        let mut variable =
            OpenVariable::open(&store.store, spec)?.with_concurrency(self.concurrency);
        if let Some((cache, namespace)) = &store.cache {
            variable = variable.with_cache(Arc::clone(cache), namespace);
        }
        let variable = Arc::new(variable);
        if let Ok(mut opened) = self.opened.lock() {
            opened.insert(key, Arc::clone(&variable));
        }
        Ok(Some(variable))
    }

    /// Places `idx` of `points` on `var` and reads every corner they need.
    fn read(
        &self,
        var: &OpenVariable,
        points: &[Point],
        idx: &[usize],
        every: i64,
        cancel: &AtomicBool,
    ) -> Result<Placed> {
        let grid = var.grid();
        let time = var.time();
        let stamps: Vec<Option<Stamp>> = idx
            .iter()
            .map(|&i| {
                let p = points[i];
                let stencil = grid.stencil(p.lat, p.lon)?;
                let (a, b, w) = bracket_every(&time, p.t, every)?;
                Some(Stamp { stencil, a, b, w })
            })
            .collect();
        let mut cells: Vec<Cell> = Vec::new();
        for stamp in stamps.iter().flatten() {
            cells.extend(corner_cells(stamp.a, &stamp.stencil));
            if stamp.b != stamp.a {
                cells.extend(corner_cells(stamp.b, &stamp.stencil));
            }
        }
        cells.sort_unstable();
        cells.dedup();
        let values = var.read_cells(&cells, cancel)?;
        Ok((stamps, values))
    }

    /// A scalar at each of `idx`.
    fn scalar(
        &self,
        var: &OpenVariable,
        points: &[Point],
        idx: &[usize],
        every: i64,
        cancel: &AtomicBool,
    ) -> Result<Vec<Option<f64>>> {
        let (stamps, values) = self.read(var, points, idx, every, cancel)?;
        Ok(stamps
            .iter()
            .map(|stamp| {
                let s = stamp.as_ref()?;
                let first = side(&s.stencil, corners_of(&values, s.a, &s.stencil));
                if s.a == s.b {
                    return first;
                }
                let second = side(&s.stencil, corners_of(&values, s.b, &s.stencil));
                lerp_time(first, second, s.w)
            })
            .collect())
    }

    /// A direction in degrees at each of `idx`, interpolated as a unit
    /// vector so 350° and 10° average to 0°, not 180°.
    fn direction(
        &self,
        var: &OpenVariable,
        points: &[Point],
        idx: &[usize],
        every: i64,
        cancel: &AtomicBool,
    ) -> Result<Vec<Option<f64>>> {
        let (stamps, values) = self.read(var, points, idx, every, cancel)?;
        let unit = |s: &Stamp, step: u64| -> Option<(f64, f64)> {
            let corners = corners_of(&values, step, &s.stencil);
            let sin = corners.map(|d| (f64::from(d).to_radians().sin()) as f32);
            let cos = corners.map(|d| (f64::from(d).to_radians().cos()) as f32);
            Some((side(&s.stencil, sin)?, side(&s.stencil, cos)?))
        };
        Ok(stamps
            .iter()
            .map(|stamp| {
                let s = stamp.as_ref()?;
                let first = unit(s, s.a);
                let (x, y) = if s.a == s.b {
                    first?
                } else {
                    let second = unit(s, s.b);
                    (
                        lerp_time(first.map(|f| f.0), second.map(|f| f.0), s.w)?,
                        lerp_time(first.map(|f| f.1), second.map(|f| f.1), s.w)?,
                    )
                };
                if x.hypot(y) < 1e-9 {
                    return None;
                }
                Some(x.atan2(y).to_degrees().rem_euclid(360.0))
            })
            .collect())
    }

    /// Sums of components at `idx`: `None` wherever any is missing.
    fn vector(
        &self,
        parts: &[(Variable, Variable)],
        points: &[Point],
        idx: &[usize],
        every: i64,
        cancel: &AtomicBool,
    ) -> Result<Option<Vectors>> {
        let mut sum: Vectors = vec![Some((0.0, 0.0)); idx.len()];
        for (u_spec, v_spec) in parts {
            let (Some(u), Some(v)) = (self.var(*u_spec)?, self.var(*v_spec)?) else {
                return Ok(None);
            };
            let us = self.scalar(&u, points, idx, every, cancel)?;
            let vs = self.scalar(&v, points, idx, every, cancel)?;
            for ((acc, u), v) in sum.iter_mut().zip(us).zip(vs) {
                *acc = match (*acc, u, v) {
                    (Some((su, sv)), Some(u), Some(v)) => Some((su + u, sv + v)),
                    _ => None,
                };
            }
        }
        Ok(Some(sum))
    }

    /// Wind: WeatherBench2 while both bracketing steps are in it, else
    /// ARCO-ERA5 (D12).
    fn wind(
        &self,
        points: &[Point],
        every: i64,
        out: &mut [EnvPoint],
        cancel: &AtomicBool,
    ) -> Result<()> {
        let wb2 = self.var(vars::WB2_U10)?;
        let (mut on_wb2, mut on_arco) = (Vec::new(), Vec::new());
        for (i, p) in points.iter().enumerate() {
            let inside = wb2.as_ref().is_some_and(|u| {
                let axis = u.time();
                bracket_every(&axis, p.t, every).is_some()
            });
            if inside {
                on_wb2.push(i);
            } else {
                on_arco.push(i);
            }
        }
        for (idx, u, v, dataset) in [
            (on_wb2, vars::WB2_U10, vars::WB2_V10, Dataset::Wb2Era5Hourly),
            (on_arco, vars::ARCO_U10, vars::ARCO_V10, Dataset::ArcoEra5),
        ] {
            if idx.is_empty() {
                continue;
            }
            let Some(found) = self.vector(&[(u, v)], points, &idx, every, cancel)? else {
                continue;
            };
            for (&i, value) in idx.iter().zip(found) {
                out[i].wind = value.map(|(u, v)| Vector { u, v, dataset });
            }
        }
        Ok(())
    }

    /// Waves from ARCO-ERA5: height as a scalar, direction as a unit vector.
    fn waves(
        &self,
        points: &[Point],
        every: i64,
        out: &mut [EnvPoint],
        cancel: &AtomicBool,
    ) -> Result<()> {
        let (Some(swh), Some(mwd)) = (self.var(vars::ARCO_SWH)?, self.var(vars::ARCO_MWD)?) else {
            return Ok(());
        };
        let idx: Vec<usize> = (0..points.len()).collect();
        let hs = self.scalar(&swh, points, &idx, every, cancel)?;
        let from = self.direction(&mwd, points, &idx, every, cancel)?;
        for ((slot, hs), from) in out.iter_mut().zip(hs).zip(from) {
            if hs.is_some() || from.is_some() {
                slot.waves = Some(Waves {
                    hs,
                    from,
                    dataset: Dataset::ArcoEra5,
                });
            }
        }
        Ok(())
    }

    /// Current from the first tier that has it (spec.md 7.5.1, D20):
    /// regional tidal reanalyses, then the global merged current (uo +
    /// utide, + Stokes drift if asked), then GlobCurrent.
    fn current(
        &self,
        points: &[Point],
        stokes: bool,
        out: &mut [EnvPoint],
        cancel: &AtomicBool,
    ) -> Result<()> {
        let hourly = Interval::Hourly.seconds();
        let mut merged = vec![
            (vars::CMEMS_UO, vars::CMEMS_VO),
            (vars::CMEMS_UTIDE, vars::CMEMS_VTIDE),
        ];
        if stokes {
            merged.push((vars::CMEMS_VSDX, vars::CMEMS_VSDY));
        }
        let tiers: [Tier; 5] = [
            (
                Dataset::CmemsNwsMy,
                vec![(vars::NWS_UO, vars::NWS_VO)],
                Some(NWS_BOX),
            ),
            (
                Dataset::CmemsIbiMy,
                vec![(vars::IBI_UO, vars::IBI_VO)],
                Some(IBI_BOX),
            ),
            (Dataset::CmemsGlobalMerged, merged, None),
            (
                Dataset::GlobCurrentMy,
                vec![(vars::GC_MY_UO, vars::GC_MY_VO)],
                None,
            ),
            (
                Dataset::GlobCurrentNrt,
                vec![(vars::GC_NRT_UO, vars::GC_NRT_VO)],
                None,
            ),
        ];
        let mut remaining: Vec<usize> = (0..points.len()).collect();
        for (dataset, parts, region) in tiers {
            let candidates: Vec<usize> = remaining
                .iter()
                .copied()
                .filter(|&i| region.is_none_or(|b| in_box(&points[i], b)))
                .collect();
            if candidates.is_empty() {
                continue;
            }
            if !self.tier_opens(dataset, &parts, cancel)? {
                continue;
            }
            // Only positions inside this tier's grid and time axis.
            let Some(first) = self.var(parts[0].0)? else {
                continue;
            };
            let (grid, axis) = (first.grid(), first.time());
            let inside: Vec<usize> = candidates
                .into_iter()
                .filter(|&i| {
                    let p = points[i];
                    grid.stencil(p.lat, p.lon).is_some() && axis.bracket(p.t).is_some()
                })
                .collect();
            if inside.is_empty() {
                continue;
            }
            let Some(found) = self.vector(&parts, points, &inside, hourly, cancel)? else {
                continue;
            };
            for (&i, value) in inside.iter().zip(found) {
                if let Some((u, v)) = value {
                    out[i].current = Some(Vector { u, v, dataset });
                }
            }
            remaining.retain(|&i| out[i].current.is_none());
            if remaining.is_empty() {
                break;
            }
        }
        Ok(())
    }
}

impl Reanalysis {
    /// Whether every array of a current tier opens. A tier whose store
    /// will not open (the archive down, a version withdrawn) is left out
    /// for [`TIER_RETRY`] with a warning, and its positions go on to the
    /// next tier, rather than failing the whole batch: wind and waves, and
    /// the other tiers, are still worth having.
    fn tier_opens(
        &self,
        dataset: Dataset,
        parts: &[(Variable, Variable)],
        cancel: &AtomicBool,
    ) -> Result<bool> {
        let recently_failed = self
            .unavailable
            .lock()
            .ok()
            .and_then(|u| u.get(&dataset).copied())
            .is_some_and(|at| at.elapsed() < TIER_RETRY);
        if recently_failed {
            return Ok(false);
        }
        for (u, v) in parts {
            for spec in [*u, *v] {
                match self.var(spec) {
                    Ok(Some(_)) => {}
                    Ok(None) => return Ok(false),
                    Err(_) if cancel.load(Ordering::SeqCst) => {
                        return Err(crate::EnvError::Cancelled);
                    }
                    Err(err) => {
                        if let Ok(mut u) = self.unavailable.lock() {
                            u.insert(dataset, std::time::Instant::now());
                        }
                        if let Ok(mut w) = self.warnings.lock() {
                            w.push(format!(
                                "the current source {} could not be opened and was left out: {err}",
                                dataset.id()
                            ));
                        }
                        return Ok(false);
                    }
                }
            }
        }
        Ok(true)
    }
}

/// Stops the stores watching a job's cancel flag when its read ends.
struct Watching<'a>(&'a Interrupt);

impl Drop for Watching<'_> {
    fn drop(&mut self) {
        self.0.watch(None);
    }
}

impl Provider for Reanalysis {
    fn sample(
        &self,
        points: &[Point],
        options: &Options,
        cancel: &Arc<AtomicBool>,
    ) -> Result<Vec<EnvPoint>> {
        let mut out = vec![EnvPoint::default(); points.len()];
        if points.is_empty() {
            return Ok(out);
        }
        self.interrupt.watch(Some(Arc::clone(cancel)));
        let _watching = Watching(&self.interrupt);
        let every = options.interval.seconds();
        let mut run = || -> Result<()> {
            self.wind(points, every, &mut out, cancel)?;
            self.waves(points, every, &mut out, cancel)?;
            self.current(points, options.stokes_drift, &mut out, cancel)
        };
        match run() {
            // A read that failed because the user cancelled is a cancel.
            Err(_) if cancel.load(Ordering::SeqCst) => Err(crate::EnvError::Cancelled),
            Err(err) => Err(err),
            Ok(()) => Ok(out),
        }
    }

    fn take_warnings(&self) -> Vec<String> {
        self.warnings
            .lock()
            .map(|mut w| std::mem::take(&mut *w))
            .unwrap_or_default()
    }
}

// ------------------------------------------------------------ estimate

/// Measured download sizes of one hour of one ERA5 variable (M3, spec.md
/// 13): wind 3.3 MB, wave height 1.8 MB, wave direction 1.7 MB.
const WIND_CHUNK_BYTES: u64 = 3_300_000;
const SWH_CHUNK_BYTES: u64 = 1_800_000;
const MWD_CHUNK_BYTES: u64 = 1_700_000;
/// One current geoChunk of one variable, about (M3).
const CURRENT_CHUNK_BYTES: u64 = 800_000;

/// WeatherBench2's time axis: hours since 1959-01-01, 561,264 of them, so
/// its last hour is 2023-01-10T23Z.
const WB2_FIRST: i64 = -347_155_200;
const WB2_LAST: i64 = WB2_FIRST + (561_264 - 1) * 3600;
/// ARCO-ERA5's time axis: hours since 1900-01-01.
const ARCO_FIRST: i64 = -2_208_988_800;

/// What a fetch is expected to download (spec.md 13, D19).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estimate {
    /// Bytes to download sampling hourly, chunks already cached left out.
    pub hourly_bytes: u64,
    /// Bytes to download sampling 3-hourly.
    pub three_hourly_bytes: u64,
    /// Bytes of the hourly fetch already in the cache.
    pub hourly_cached_bytes: u64,
}

/// The hours of the ERA5 steps bracketing `t` every `every` seconds.
fn era5_hours(t: i64, every: i64) -> [i64; 2] {
    let a = t - t.rem_euclid(every);
    if a == t { [a, a] } else { [a, a + every] }
}

/// Expected download for sampling `points`: the ERA5 hours they need (wind
/// from WeatherBench2 or ARCO-ERA5, waves from ARCO-ERA5), less those the
/// cache already holds, plus the currents.
///
/// An estimate, not a promise. Its approximations:
/// - **Sizes** are the per-chunk averages measured in M3; real chunks vary
///   with the weather (compression).
/// - **The WeatherBench2 → ARCO boundary** is decided per hour here, while
///   the sampler decides per position (both bracketing hours in
///   WeatherBench2); a position in the last hour before the switch is
///   counted against WeatherBench2 but read from ARCO.
/// - **Currents** are counted as about one 0.8 MB chunk per variable for
///   four variables per 1.3° × 0.7° box and half year crossed, whatever the
///   tier: the regional stores chunk finer boxes over longer spans, Stokes
///   drift adds two variables, GlobCurrent has two, and land and positions
///   answered by an earlier tier cost nothing. Currents are not checked
///   against the cache.
/// - **The cache** check is by key presence at estimate time; eviction
///   during the fetch can make some of it download again.
pub fn estimate(points: &[Point], cache: Option<&ChunkCache>) -> Estimate {
    let mut out = Estimate::default();
    for (every, total) in [
        (3600, &mut out.hourly_bytes),
        (3 * 3600, &mut out.three_hourly_bytes),
    ] {
        let mut hours: Vec<i64> = points.iter().flat_map(|p| era5_hours(p.t, every)).collect();
        hours.sort_unstable();
        hours.dedup();
        let mut cached = 0;
        for hour in hours {
            let (wind_ds, wind_index) = if (WB2_FIRST..=WB2_LAST).contains(&hour) {
                (Dataset::Wb2Era5Hourly, (hour - WB2_FIRST) / 3600)
            } else {
                (Dataset::ArcoEra5, (hour - ARCO_FIRST) / 3600)
            };
            let arco_index = (hour - ARCO_FIRST) / 3600;
            let chunks = [
                (wind_ds, vars::WB2_U10.array, wind_index, WIND_CHUNK_BYTES),
                (wind_ds, vars::WB2_V10.array, wind_index, WIND_CHUNK_BYTES),
                (
                    Dataset::ArcoEra5,
                    vars::ARCO_SWH.array,
                    arco_index,
                    SWH_CHUNK_BYTES,
                ),
                (
                    Dataset::ArcoEra5,
                    vars::ARCO_MWD.array,
                    arco_index,
                    MWD_CHUNK_BYTES,
                ),
            ];
            for (dataset, array, index, bytes) in chunks {
                let key = format!("{array}/{index}.0.0");
                if cache.is_some_and(|c| c.contains(dataset.id(), &key)) {
                    cached += bytes;
                } else {
                    *total += bytes;
                }
            }
        }
        if every == 3600 {
            out.hourly_cached_bytes = cached;
        }
    }
    // Currents: a geoChunk is about 16 × 8 cells of 1/12° (1.3° × 0.7°) and
    // half a year; four variables (uo, vo, utide, vtide).
    let mut boxes: Vec<(i64, i64, i64)> = points
        .iter()
        .map(|p| {
            (
                (p.lat / 0.7).floor() as i64,
                (p.lon / 1.3).floor() as i64,
                p.t.div_euclid(182 * 86_400),
            )
        })
        .collect();
    boxes.sort_unstable();
    boxes.dedup();
    let current = boxes.len() as u64 * 4 * CURRENT_CHUNK_BYTES;
    out.hourly_bytes += current;
    out.three_hourly_bytes += current;
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn axis() -> TimeAxis {
        TimeAxis {
            first: 0,
            step: 3600,
            len: 48,
        }
    }

    #[test]
    fn a_three_hourly_bracket_uses_steps_three_hours_apart() {
        let t = 4 * 3600 + 1800; // 04:30
        assert_eq!(bracket_every(&axis(), t, 3600), Some((4, 5, 0.5)));
        // 03:00 and 06:00, 1.5 h of 3 h on.
        assert_eq!(bracket_every(&axis(), t, 3 * 3600), Some((3, 6, 0.5)));
        // On a coarse step exactly: that step alone.
        assert_eq!(
            bracket_every(&axis(), 6 * 3600, 3 * 3600),
            Some((6, 6, 0.0))
        );
        // Past the last coarse step (45:00) the hourly steps are used.
        let late = 46 * 3600 + 1800;
        assert_eq!(bracket_every(&axis(), late, 3 * 3600), Some((46, 47, 0.5)));
        assert_eq!(bracket_every(&axis(), -1, 3 * 3600), None);
    }

    #[test]
    fn intervals_round_trip_through_seconds() {
        for i in [Interval::Hourly, Interval::ThreeHourly] {
            assert_eq!(Interval::from_seconds(i.seconds()), Some(i));
        }
        assert_eq!(Interval::from_seconds(7200), None);
    }

    #[test]
    fn the_regional_boxes_take_either_longitude_convention() {
        let p = Point {
            t: 0,
            lat: 50.0,
            lon: 355.0,
        };
        assert!(in_box(&p, NWS_BOX));
        assert!(in_box(&p, IBI_BOX));
        let far = Point { lon: 60.0, ..p };
        assert!(!in_box(&far, NWS_BOX));
    }

    /// Hand-counted: a 24-hour track sampled every 10 minutes needs 25
    /// hours hourly (00Z to 24Z) and 9 three-hourly (00Z to 24Z); each hour
    /// is 3.3 × 2 + 1.8 + 1.7 = 10.1 MB of ERA5. The whole track sits in one
    /// current box: 4 × 0.8 MB.
    #[test]
    fn the_estimate_counts_era5_hours_and_current_boxes() {
        let t0 = crate::time::parse_utc("2020-07-27T00:00Z").expect("a time");
        let points: Vec<Point> = (0..=144)
            .map(|k| Point {
                t: t0 + k * 600,
                lat: 50.1,
                lon: -4.9,
            })
            .collect();
        let e = estimate(&points, None);
        let current = 4 * CURRENT_CHUNK_BYTES;
        assert_eq!(e.hourly_bytes, 25 * 10_100_000 + current);
        assert_eq!(e.three_hourly_bytes, 9 * 10_100_000 + current);
        assert_eq!(e.hourly_cached_bytes, 0);
    }

    /// A chunk in the cache is not counted again: the second boat of a race.
    #[test]
    fn cached_hours_are_not_counted() {
        let dir = std::env::temp_dir().join(format!("pe-estimate-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let cache = ChunkCache::open(&dir, 1 << 30).expect("opens");
        let t = crate::time::parse_utc("2020-07-27T12:00Z").expect("a time");
        // 539724: the WB2 index of that hour (time.rs test).
        cache
            .put("wb2-era5-1h", "10m_u_component_of_wind/539724.0.0", b"x")
            .expect("put");
        let e = estimate(
            &[Point {
                t,
                lat: 50.0,
                lon: -5.0,
            }],
            Some(&cache),
        );
        assert_eq!(e.hourly_cached_bytes, WIND_CHUNK_BYTES);
        assert_eq!(
            e.hourly_bytes,
            WIND_CHUNK_BYTES + SWH_CHUNK_BYTES + MWD_CHUNK_BYTES + 4 * CURRENT_CHUNK_BYTES
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
