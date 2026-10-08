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

use crate::dataset::{
    Cell, Dataset, OpenVariable, Variable, corner_cells, corners_of, lerp_time, vars,
};
use crate::error::Result;
use crate::grid::Stencil;
use crate::http::Interrupt;
use crate::memory::BlockCache;
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
}

impl Interval {
    /// The spacing, seconds.
    pub fn seconds(self) -> i64 {
        match self {
            Self::Hourly => 3600,
        }
    }

    /// The interval of this spacing, if it is one.
    pub fn from_seconds(seconds: i64) -> Option<Self> {
        [Self::Hourly].into_iter().find(|i| i.seconds() == seconds)
    }
}

/// Which of wind, waves and current a read asks for. A fetch for a track
/// asks for all three; a GRIB export only for what the user ticked, and
/// for the current apart from the rest (spec.md 7.8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Parts {
    /// 10 m wind.
    pub wind: bool,
    /// Wave height and direction.
    pub waves: bool,
    /// Surface current.
    pub current: bool,
}

impl Parts {
    /// Everything: what a track's fetch reads.
    pub const ALL: Self = Self {
        wind: true,
        waves: true,
        current: true,
    };
}

impl Default for Parts {
    fn default() -> Self {
        Self::ALL
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
    /// What is read; the rest is left `None`.
    pub parts: Parts,
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
    /// Mean wave period, seconds.
    pub period_s: Option<f64>,
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

    /// Called once with every position a fetch will ask for, before its
    /// batches, so the provider can start what they share (the ERA5
    /// chunks' heads, read side by side). Nothing it does changes a
    /// batch's answer.
    ///
    /// # Errors
    /// [`crate::EnvError::Cancelled`] once `cancel` is set.
    fn prepare(
        &self,
        _points: &[Point],
        _options: &Options,
        _cancel: &Arc<AtomicBool>,
    ) -> Result<()> {
        Ok(())
    }

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
    /// Over HTTPS (the app).
    Http {
        /// Per-request timeout (spec.md 3.4).
        timeout: Duration,
    },
    /// From recorded stores in folders (tests). A dataset without a folder
    /// is unavailable, as if it had no data anywhere.
    Dirs(BTreeMap<Dataset, PathBuf>),
    /// Over HTTP from other addresses than the archives' (tests, and the
    /// UX suite's local server: a loopback address only, checked by the
    /// caller). A dataset without an address is unavailable.
    Urls {
        /// Per-request timeout.
        timeout: Duration,
        /// Each dataset's store root.
        urls: BTreeMap<Dataset, String>,
    },
}

/// The real provider: the archives of spec.md 7.5 and 7.5.1.
///
/// Opened stores and arrays are kept for the life of the provider: each
/// open costs about a second of metadata requests (M3), which a job over
/// several tracks would otherwise pay per track.
pub struct Reanalysis {
    whirlwind: Option<crate::whirlwind::Whirlwind>,
    access: Access,
    /// What this session has downloaded: every track of every event reads
    /// through it, so the second boat of a race costs little. In memory
    /// only (invariant 3).
    memory: Arc<BlockCache>,
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

/// What [`Reanalysis::era5_wants`] plans: the variables to read with their
/// positions, the wind's positions and dataset per (u, v) pair, and whether
/// the last two are the waves.
type Era5Plan = (
    Vec<(Arc<OpenVariable>, Vec<usize>)>,
    Vec<(Vec<usize>, Dataset)>,
    [Option<usize>; 3],
);

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
pub(crate) struct Stamp {
    pub(crate) stencil: Stencil,
    pub(crate) a: u64,
    pub(crate) b: u64,
    pub(crate) w: f64,
}

/// The steps either side of `t` on `axis`, taking only every `every`
/// seconds (aligned on UTC midnight), and the weight of the later one.
/// Falls back to every step where the coarse steps are not on the axis
/// (its end, or an axis whose spacing does not divide `every`).
pub(crate) fn bracket_every(axis: &TimeAxis, t: i64, every: i64) -> Option<(u64, u64, f64)> {
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

/// Places `idx` of `points` on `var`: each position's stencil and time
/// steps, and every cell they need.
fn place(
    var: &OpenVariable,
    points: &[Point],
    idx: &[usize],
    every: i64,
) -> (Vec<Option<Stamp>>, Vec<Cell>) {
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
    (stamps, cells)
}

/// A scalar at each placed position.
pub(crate) fn scalar_of((stamps, values): &Placed) -> Vec<Option<f64>> {
    stamps
        .iter()
        .map(|stamp| {
            let s = stamp.as_ref()?;
            let first = side(&s.stencil, corners_of(values, s.a, &s.stencil));
            if s.a == s.b {
                return first;
            }
            let second = side(&s.stencil, corners_of(values, s.b, &s.stencil));
            lerp_time(first, second, s.w)
        })
        .collect()
}

/// A direction in degrees at each placed position, interpolated as a unit
/// vector so 350° and 10° average to 0°, not 180°.
pub(crate) fn direction_of((stamps, values): &Placed) -> Vec<Option<f64>> {
    let unit = |s: &Stamp, step: u64| -> Option<(f64, f64)> {
        let corners = corners_of(values, step, &s.stencil);
        let sin = corners.map(|d| (f64::from(d).to_radians().sin()) as f32);
        let cos = corners.map(|d| (f64::from(d).to_radians().cos()) as f32);
        Some((side(&s.stencil, sin)?, side(&s.stencil, cos)?))
    };
    stamps
        .iter()
        .map(|stamp| {
            let s = stamp.as_ref()?;
            // On a grid node at an archive hour the direction is the
            // archive's, read as it is rather than through sin, cos and
            // atan2, whose last bit differs between platforms' maths
            // libraries: a GRIB export of these nodes is byte-identical
            // everywhere (invariant 5).
            if s.a == s.b && s.stencil.lat_frac == 0.0 && s.stencil.lon_frac == 0.0 {
                let node = corners_of(values, s.a, &s.stencil)[0];
                return node.is_finite().then(|| f64::from(node).rem_euclid(360.0));
            }
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
        .collect()
}

/// The sum of (u, v) pairs read in order (u, v, u, v, …) at `n` positions:
/// `None` wherever any part is missing.
fn sum_vectors(read: &[Placed], n: usize) -> Vectors {
    let mut sum: Vectors = vec![Some((0.0, 0.0)); n];
    for pair in read.chunks(2) {
        let [u, v] = pair else {
            continue;
        };
        for ((acc, u), v) in sum.iter_mut().zip(scalar_of(u)).zip(scalar_of(v)) {
            *acc = match (*acc, u, v) {
                (Some((su, sv)), Some(u), Some(v)) => Some((su + u, sv + v)),
                _ => None,
            };
        }
    }
    sum
}

impl Reanalysis {
    /// The Whirlwind archive, using its own bounded async pool.
    pub fn whirlwind(
        source: crate::whirlwind::Source,
        credentials: Option<crate::whirlwind::Credentials>,
        timeout: Duration,
        memory: Arc<BlockCache>,
    ) -> Result<Self> {
        let whirlwind = crate::whirlwind::Whirlwind::new(
            source,
            credentials,
            timeout,
            Arc::clone(&memory),
            crate::whirlwind::CONCURRENCY,
        )?;
        let mut reader = Self::http(timeout, memory, crate::whirlwind::CONCURRENCY);
        reader.whirlwind = Some(whirlwind);
        Ok(reader)
    }

    /// Persist validated Whirlwind chunks across routes and sessions.
    pub fn with_whirlwind_cache(mut self, cache: Arc<crate::whirlwind::DiskCache>) -> Self {
        self.whirlwind = self.whirlwind.map(|reader| reader.with_disk_cache(cache));
        self
    }

    /// Download accounting for the active source.
    pub fn estimate(&self, points: &[Point]) -> Result<Estimate> {
        match &self.whirlwind {
            Some(w) => w.estimate(points),
            None => Ok(estimate(points, Some(&self.memory))),
        }
    }

    /// A provider reading the archives over HTTPS, keeping what it
    /// downloads in `memory`, with at most `concurrency` chunk reads at once
    /// (spec.md 3.4).
    pub fn http(timeout: Duration, memory: Arc<BlockCache>, concurrency: usize) -> Self {
        Self::new(Access::Http { timeout }, concurrency).with_memory(memory)
    }

    /// A provider with its own memory of the default size.
    pub fn new(access: Access, concurrency: usize) -> Self {
        Self {
            access,
            whirlwind: None,
            memory: BlockCache::new(crate::memory::DEFAULT_LIMIT),
            concurrency: concurrency.max(1),
            stores: Mutex::new(BTreeMap::new()),
            opened: Mutex::new(BTreeMap::new()),
            interrupt: Arc::new(Interrupt::default()),
            unavailable: Mutex::new(BTreeMap::new()),
            warnings: Mutex::new(Vec::new()),
        }
    }

    /// The same provider keeping its downloads in `memory`.
    pub fn with_memory(mut self, memory: Arc<BlockCache>) -> Self {
        self.memory = memory;
        self
    }

    /// `(requests, bytes)` sent to and received from the archives so far.
    pub fn net_totals(&self) -> (u64, u64) {
        if let Some(w) = &self.whirlwind {
            return w.net_totals();
        }
        self.stores.lock().map_or((0, 0), |stores| {
            stores
                .values()
                .filter_map(|s| s.net.as_ref())
                .map(|n| n.snapshot())
                .fold((0, 0), |(r, b), (r1, b1, _)| (r + r1, b + b1))
        })
    }

    /// What it keeps of its downloads.
    pub fn memory(&self) -> &Arc<BlockCache> {
        &self.memory
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
            Access::Http { timeout } => {
                open_http_interruptible(dataset.url(), *timeout, Arc::clone(&self.interrupt))?
            }
            Access::Dirs(dirs) => match dirs.get(&dataset) {
                Some(dir) => open_dir(dir)?,
                None => return Ok(None),
            },
            Access::Urls { timeout, urls } => match urls.get(&dataset) {
                Some(url) => open_http_interruptible(url, *timeout, Arc::clone(&self.interrupt))?,
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
        let variable = Arc::new(
            OpenVariable::open(&store, spec)?
                .with_concurrency(self.concurrency)
                .with_memory(Arc::clone(&self.memory)),
        );
        if let Ok(mut opened) = self.opened.lock() {
            opened.insert(key, Arc::clone(&variable));
        }
        Ok(Some(variable))
    }

    /// Places `idx` of `points` on each variable and reads every corner
    /// they need, all variables' chunks on one pool.
    fn read_all(
        &self,
        wants: &[(Arc<OpenVariable>, Vec<usize>)],
        points: &[Point],
        every: i64,
        cancel: &AtomicBool,
    ) -> Result<Vec<Placed>> {
        let placed: Vec<(Vec<Option<Stamp>>, Vec<Cell>)> = wants
            .iter()
            .map(|(var, idx)| place(var, points, idx, every))
            .collect();
        let requests: Vec<(&OpenVariable, &[Cell])> = wants
            .iter()
            .zip(&placed)
            .map(|((var, _), (_, cells))| (var.as_ref(), cells.as_slice()))
            .collect();
        let values = crate::dataset::read_cells_of(&requests, cancel, self.concurrency)?;
        Ok(placed
            .into_iter()
            .zip(values)
            .map(|((stamps, _), values)| (stamps, values))
            .collect())
    }

    /// Sums of components at `idx`: `None` wherever any is missing, and
    /// `None` in all when a part's store is unavailable.
    fn vector(
        &self,
        parts: &[(Variable, Variable)],
        points: &[Point],
        idx: &[usize],
        every: i64,
        cancel: &AtomicBool,
    ) -> Result<Option<Vectors>> {
        let mut wants = Vec::new();
        for (u_spec, v_spec) in parts {
            let (Some(u), Some(v)) = (self.var(*u_spec)?, self.var(*v_spec)?) else {
                return Ok(None);
            };
            wants.push((u, idx.to_vec()));
            wants.push((v, idx.to_vec()));
        }
        let read = self.read_all(&wants, points, every, cancel)?;
        Ok(Some(sum_vectors(&read, idx.len())))
    }

    /// What wind and waves need read: each variable with the positions it
    /// is read at (wind u, v pairs by dataset, then wave height and
    /// direction), the wind's positions and dataset per pair, and whether
    /// the waves are among them. Wind comes from WeatherBench2 while both
    /// bracketing steps are in it, else ARCO-ERA5 (D12).
    fn era5_wants(&self, points: &[Point], every: i64, parts: Parts) -> Result<Era5Plan> {
        let mut wants: Vec<(Arc<OpenVariable>, Vec<usize>)> = Vec::new();
        let mut winds: Vec<(Vec<usize>, Dataset)> = Vec::new();
        if parts.wind {
            self.wind_wants(points, every, &mut wants, &mut winds)?;
        }
        let all: Vec<usize> = (0..points.len()).collect();
        let mut waves = [None; 3];
        if parts.waves {
            for (index, spec) in [vars::ARCO_SWH, vars::ARCO_MWD, vars::ARCO_MWP]
                .into_iter()
                .enumerate()
            {
                if let Some(var) = self.var(spec)? {
                    waves[index] = Some(wants.len());
                    wants.push((var, all.clone()));
                }
            }
        }
        Ok((wants, winds, waves))
    }

    /// The wind part of [`Self::era5_wants`].
    fn wind_wants(
        &self,
        points: &[Point],
        every: i64,
        wants: &mut Vec<(Arc<OpenVariable>, Vec<usize>)>,
        winds: &mut Vec<(Vec<usize>, Dataset)>,
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
            let (Some(u), Some(v)) = (self.var(u)?, self.var(v)?) else {
                continue;
            };
            wants.push((u, idx.clone()));
            wants.push((v, idx.clone()));
            winds.push((idx, dataset));
        }
        Ok(())
    }

    /// Wind and waves together, their chunks on one pool: wind u and v,
    /// wave height as a scalar and direction as a unit vector.
    fn era5(
        &self,
        points: &[Point],
        every: i64,
        parts: Parts,
        out: &mut [EnvPoint],
        cancel: &AtomicBool,
    ) -> Result<()> {
        if !parts.wind && !parts.waves {
            return Ok(());
        }
        let (wants, winds, waves) = self.era5_wants(points, every, parts)?;
        let read = self.read_all(&wants, points, every, cancel)?;
        for (k, (idx, dataset)) in winds.iter().enumerate() {
            let found = sum_vectors(&read[2 * k..2 * k + 2], idx.len());
            for (&i, value) in idx.iter().zip(found) {
                out[i].wind = value.map(|(u, v)| Vector {
                    u,
                    v,
                    dataset: *dataset,
                });
            }
        }
        if waves.iter().any(Option::is_some) {
            let values = |index: Option<usize>, direction: bool| {
                index.map_or_else(
                    || vec![None; points.len()],
                    |i| {
                        if direction {
                            direction_of(&read[i])
                        } else {
                            scalar_of(&read[i])
                        }
                    },
                )
            };
            let hs = values(waves[0], false);
            let from = values(waves[1], true);
            let period = values(waves[2], false);
            for (((slot, hs), from), period_s) in out.iter_mut().zip(hs).zip(from).zip(period) {
                if hs.is_some() || from.is_some() || period_s.is_some() {
                    slot.waves = Some(Waves {
                        hs,
                        from,
                        period_s,
                        dataset: Dataset::ArcoEra5,
                    });
                }
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
        cancel: &AtomicBool,
    ) -> Result<Vec<Option<Vector>>> {
        let mut out: Vec<Option<Vector>> = vec![None; points.len()];
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
                    out[i] = Some(Vector { u, v, dataset });
                }
            }
            remaining.retain(|&i| out[i].is_none());
            if remaining.is_empty() {
                break;
            }
        }
        Ok(out)
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
        if let Some(w) = &self.whirlwind {
            return w.sample(points, options, cancel);
        }
        let mut out = vec![EnvPoint::default(); points.len()];
        if points.is_empty() {
            return Ok(out);
        }
        self.interrupt.watch(Some(Arc::clone(cancel)));
        let _watching = Watching(&self.interrupt);
        let every = options.interval.seconds();
        // Wind and waves come from Google Cloud, currents from Copernicus
        // Marine: the two are read side by side, each archive with at most
        // `concurrency` requests in flight, so one's latency hides the
        // other's.
        let mut run = || -> Result<()> {
            let parts = options.parts;
            let (currents, ()) = crate::parallel::try_join(
                || {
                    if parts.current {
                        self.current(points, options.stokes_drift, cancel)
                    } else {
                        Ok(vec![None; points.len()])
                    }
                },
                || self.era5(points, every, parts, &mut out, cancel),
            )?;
            for (slot, current) in out.iter_mut().zip(currents) {
                slot.current = current;
            }
            Ok(())
        };
        match run() {
            // A read that failed because the user cancelled is a cancel.
            Err(_) if cancel.load(Ordering::SeqCst) => Err(crate::EnvError::Cancelled),
            Err(err) => Err(err),
            Ok(()) => Ok(out),
        }
    }

    /// Reads the head of every wind and wave chunk the positions need, on
    /// one pool, before the batches: a batch then waits for one round trip
    /// per chunk (its blocks) instead of two. A failed head read is left to
    /// the batch that needs it to report.
    fn prepare(&self, points: &[Point], options: &Options, cancel: &Arc<AtomicBool>) -> Result<()> {
        if let Some(w) = &self.whirlwind {
            return w.prepare(points, options, cancel);
        }
        if points.is_empty() {
            return Ok(());
        }
        self.interrupt.watch(Some(Arc::clone(cancel)));
        let _watching = Watching(&self.interrupt);
        let every = options.interval.seconds();
        let heads = || -> Result<usize> {
            let (wants, ..) = self.era5_wants(points, every, options.parts)?;
            let placed: Vec<Vec<Cell>> = wants
                .iter()
                .map(|(var, idx)| place(var, points, idx, every).1)
                .collect();
            let requests: Vec<(&OpenVariable, &[Cell])> = wants
                .iter()
                .zip(&placed)
                .map(|((var, _), cells)| (var.as_ref(), cells.as_slice()))
                .collect();
            crate::dataset::prefetch_heads(&requests, cancel, self.concurrency)
        };
        match heads() {
            Err(_) if cancel.load(Ordering::SeqCst) => Err(crate::EnvError::Cancelled),
            _ => Ok(()),
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

/// Compressed bytes of each of the eight blocks of one ERA5 global field,
/// read from the block offsets of one hour of each (2026-09-28; wind:
/// WeatherBench2 u10 2020-07-27T12Z, waves: ARCO-ERA5 hour 1,100,000). A
/// block is 131,072 values, about 91 rows from the north. Wave blocks over
/// the Arctic ice and Antarctica are mostly land (NaN) and compress to
/// little.
const WIND_BLOCKS: [u64; 8] = [
    402_870, 432_731, 433_423, 428_413, 421_244, 420_944, 414_999, 371_403,
];
const SWH_BLOCKS: [u64; 8] = [
    74_490, 173_816, 241_555, 306_573, 309_805, 340_609, 284_882, 3_439,
];
const MWD_BLOCKS: [u64; 8] = [
    74_127, 165_715, 240_127, 295_265, 296_882, 312_942, 260_184, 3_406,
];
const MWP_BLOCKS: [u64; 8] = [70267, 168068, 238720, 296583, 295453, 319912, 261216, 3480];
/// Values per ERA5 block (524,288 bytes of float32), and the grid.
const ERA5_BLOCK_VALUES: usize = 131_072;
const ERA5_ROWS: usize = 721;
const ERA5_COLS: usize = 1440;

/// WeatherBench2's time axis: hours since 1959-01-01, 561,264 of them, so
/// its last hour is 2023-01-10T23Z.
const WB2_FIRST: i64 = -347_155_200;
const WB2_LAST: i64 = WB2_FIRST + (561_264 - 1) * 3600;
/// ARCO-ERA5's time axis: hours since 1900-01-01.
const ARCO_FIRST: i64 = -2_208_988_800;
/// The global merged current starts on 2020-11-01.
const MERGED_FIRST: i64 = 1_604_188_800;

/// How one current tier's geoChunks divide space and time, for the
/// estimate: the box one chunk covers (degrees), the hours one block of it
/// covers, a block's typical compressed size (measured 2026-09-28 from
/// block offsets of one chunk each) and the variables read.
struct CurrentTier {
    lat0: f64,
    lon0: f64,
    box_lat: f64,
    box_lon: f64,
    block_hours: i64,
    block_bytes: u64,
    variables: u64,
}

/// NW Shelf: 4 × 4 cells of 1/15° × 1/9°, int16, 8,192 hours a block.
const NWS_TIER: CurrentTier = CurrentTier {
    lat0: 40.0,
    lon0: -20.0,
    box_lat: 4.0 / 15.0,
    box_lon: 4.0 / 9.0,
    block_hours: 8192,
    block_bytes: 150_000,
    variables: 2,
};
/// IBI: 4 × 4 cells of 1/36°, float32, 8,192 hours a block.
const IBI_TIER: CurrentTier = CurrentTier {
    lat0: 26.0,
    lon0: -19.0,
    box_lat: 4.0 / 36.0,
    box_lon: 4.0 / 36.0,
    block_hours: 8192,
    block_bytes: 430_000,
    variables: 2,
};
/// Global merged: 16 × 8 cells of 1/12°, float32, 1,024 hours a block;
/// uo, vo, utide, vtide.
const MERGED_TIER: CurrentTier = CurrentTier {
    lat0: -80.0,
    lon0: -180.0,
    box_lat: 16.0 / 12.0,
    box_lon: 8.0 / 12.0,
    block_hours: 1024,
    block_bytes: 190_000,
    variables: 4,
};
/// GlobCurrent: 8 × 4 cells of 0.25°, int16, 4,096 hours a block.
const GLOBCURRENT_TIER: CurrentTier = CurrentTier {
    lat0: -90.0,
    lon0: -180.0,
    box_lat: 2.0,
    box_lon: 1.0,
    block_hours: 4096,
    block_bytes: 100_000,
    variables: 2,
};

/// What a fetch is expected to download (spec.md 13, D19, D27).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Estimate {
    /// Bytes to download sampling hourly, blocks already in memory left out.
    pub hourly_bytes: u64,
    /// Bytes of the hourly fetch already downloaded this session.
    pub hourly_cached_bytes: u64,
}

/// The hours of the ERA5 steps bracketing `t` every `every` seconds.
fn era5_hours(t: i64, every: i64) -> [i64; 2] {
    let a = t - t.rem_euclid(every);
    if a == t { [a, a] } else { [a, a + every] }
}

/// The ERA5 blocks holding the four stencil corners around a position.
fn era5_blocks(lat: f64, lon: f64) -> Vec<usize> {
    let row = ((90.0 - lat) / 0.25)
        .floor()
        .clamp(0.0, (ERA5_ROWS - 1) as f64) as usize;
    let col = (lon.rem_euclid(360.0) / 0.25).floor() as usize % ERA5_COLS;
    let mut blocks: Vec<usize> = [row, (row + 1).min(ERA5_ROWS - 1)]
        .into_iter()
        .flat_map(|r| [col, (col + 1) % ERA5_COLS].map(|c| (r * ERA5_COLS + c) / ERA5_BLOCK_VALUES))
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    blocks
}

/// Expected download for sampling `points`: for each ERA5 hour they need
/// (wind from WeatherBench2 or ARCO-ERA5, waves from ARCO-ERA5), each
/// chunk's first bytes and the blocks holding the positions' rows, less
/// what `memory` already holds; plus the current blocks.
///
/// An estimate, not a promise. Its approximations:
/// - **Sizes** are one hour's measured block sizes; real blocks vary with
///   the weather (compression).
/// - **The WeatherBench2 → ARCO boundary** is decided per hour here, while
///   the sampler decides per position (both bracketing hours in
///   WeatherBench2).
/// - **Currents** are counted for the first tier whose box holds the
///   position (NW Shelf, IBI, the global merged current from 2020-11, else
///   GlobCurrent), one typical block per variable per box and block of
///   hours crossed; land, positions a tier passes on, Stokes drift and
///   chunk edges are not counted, nor is memory.
pub fn estimate(points: &[Point], memory: Option<&BlockCache>) -> Estimate {
    use crate::dataset::HEAD_REQUEST;
    use crate::memory::Part;
    let mut out = Estimate::default();
    // (hour, block) pairs, then each variable's chunk for that hour.
    let mut wanted: Vec<(i64, usize)> = points
        .iter()
        .flat_map(|p| {
            let blocks = era5_blocks(p.lat, p.lon);
            era5_hours(p.t, 3600)
                .into_iter()
                .flat_map(move |h| blocks.clone().into_iter().map(move |b| (h, b)))
        })
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut hours: Vec<i64> = wanted.iter().map(|(h, _)| *h).collect();
    hours.dedup();
    let mut cached = 0;
    let chunks = |hour: i64| {
        let (wind_ds, wind_index) = if (WB2_FIRST..=WB2_LAST).contains(&hour) {
            (Dataset::Wb2Era5Hourly, (hour - WB2_FIRST) / 3600)
        } else {
            (Dataset::ArcoEra5, (hour - ARCO_FIRST) / 3600)
        };
        let arco_index = (hour - ARCO_FIRST) / 3600;
        [
            (wind_ds, vars::WB2_U10.array, wind_index, &WIND_BLOCKS),
            (wind_ds, vars::WB2_V10.array, wind_index, &WIND_BLOCKS),
            (
                Dataset::ArcoEra5,
                vars::ARCO_SWH.array,
                arco_index,
                &SWH_BLOCKS,
            ),
            (
                Dataset::ArcoEra5,
                vars::ARCO_MWD.array,
                arco_index,
                &MWD_BLOCKS,
            ),
            (
                Dataset::ArcoEra5,
                vars::ARCO_MWP.array,
                arco_index,
                &MWP_BLOCKS,
            ),
        ]
    };
    let held = |dataset: Dataset, key: &str, part: Part| {
        memory.is_some_and(|m| m.contains(dataset.id(), key, part))
    };
    for &hour in &hours {
        for (dataset, array, index, _) in chunks(hour) {
            let key = format!("{array}/{index}.0.0");
            if held(dataset, &key, Part::Head) {
                cached += HEAD_REQUEST;
            } else {
                out.hourly_bytes += HEAD_REQUEST;
            }
        }
    }
    for &(hour, block) in &wanted {
        for (dataset, array, index, sizes) in chunks(hour) {
            let key = format!("{array}/{index}.0.0");
            let bytes = sizes.get(block).copied().unwrap_or_default();
            if held(dataset, &key, Part::Block(block as u32)) {
                cached += bytes;
            } else {
                out.hourly_bytes += bytes;
            }
        }
    }
    out.hourly_cached_bytes = cached;
    let current = current_bytes(points);
    out.hourly_bytes += current;
    out
}

/// What a reanalysis GRIB export is expected to download, by part (spec.md
/// 7.8, 13).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ExportEstimate {
    /// Wind: each hour's chunk heads and the blocks holding the area's rows.
    pub wind_bytes: u64,
    /// Wave height and direction, likewise.
    pub waves_bytes: u64,
    /// The current tiers' blocks over the area.
    pub current_bytes: u64,
    /// Wind bytes already in memory this session (not counted above).
    pub wind_cached_bytes: u64,
    /// Wave bytes already in memory this session (not counted above).
    pub waves_cached_bytes: u64,
}

/// A rectangle of latitude and longitude: an export's area.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Area {
    /// Northern edge, degrees.
    pub north: f64,
    /// Southern edge, degrees.
    pub south: f64,
    /// Western edge, degrees, any convention.
    pub west: f64,
    /// Eastward width, degrees (0 to 360).
    pub width: f64,
}

impl Area {
    /// The longitude intervals of the area in [−180, 180): one, or two
    /// when it crosses the antimeridian (the first ending a hair short of
    /// 180°, which is the second's −180°).
    fn lon_segments(&self) -> Vec<(f64, f64)> {
        const EDGE: f64 = 180.0 - 1e-9;
        if self.width >= 360.0 {
            return vec![(-180.0, EDGE)];
        }
        let west = (self.west + 180.0).rem_euclid(360.0) - 180.0;
        let east = west + self.width.max(0.0);
        if east < 180.0 {
            vec![(west, east)]
        } else {
            vec![(west, EDGE), (-180.0, east - 360.0)]
        }
    }
}

/// The ERA5 blocks holding an area's stencils: for every row it covers
/// (and the one below), the blocks from its first to its last column, the
/// row split in two where the area crosses 0°E. At most 722 rows, whatever
/// the area.
fn era5_area_blocks(area: &Area) -> Vec<usize> {
    let row = |lat: f64| {
        ((90.0 - lat) / 0.25)
            .floor()
            .clamp(0.0, (ERA5_ROWS - 1) as f64) as usize
    };
    let (top, bottom) = (row(area.north), (row(area.south) + 1).min(ERA5_ROWS - 1));
    let first = (area.west.rem_euclid(360.0) / 0.25).floor() as usize % ERA5_COLS;
    let cols = ((area.width.max(0.0) / 0.25).ceil() as usize + 2).min(ERA5_COLS);
    let segments: Vec<(usize, usize)> = if first + cols <= ERA5_COLS {
        vec![(first, first + cols - 1)]
    } else {
        vec![(first, ERA5_COLS - 1), (0, first + cols - ERA5_COLS - 1)]
    };
    let mut blocks = std::collections::BTreeSet::new();
    for r in top..=bottom {
        for &(a, b) in &segments {
            let (lo, hi) = (
                (r * ERA5_COLS + a) / ERA5_BLOCK_VALUES,
                (r * ERA5_COLS + b) / ERA5_BLOCK_VALUES,
            );
            blocks.extend(lo..=hi);
        }
    }
    blocks.into_iter().collect()
}

/// Chunk boxes of `tier` that the part of `area` inside `region` (all of
/// it for `None`) touches, counted from the extents: no position is
/// placed.
fn tier_boxes(tier: &CurrentTier, area: &Area, region: Option<([f64; 2], [f64; 2])>) -> u64 {
    let (lat_lo, lat_hi, lon_bounds) = match region {
        Some((lat, lon)) => (area.south.max(lat[0]), area.north.min(lat[1]), Some(lon)),
        None => (area.south, area.north, None),
    };
    if lat_lo > lat_hi {
        return 0;
    }
    let index = |x: f64, x0: f64, size: f64| ((x - x0) / size).floor() as i64;
    let rows = (index(lat_hi, tier.lat0, tier.box_lat) - index(lat_lo, tier.lat0, tier.box_lat) + 1)
        .max(0) as u64;
    let per_turn = (360.0 / tier.box_lon).ceil() as u64;
    let mut cols = 0u64;
    for (a, b) in area.lon_segments() {
        let (a, b) = match lon_bounds {
            Some(lon) => (a.max(lon[0]), b.min(lon[1])),
            None => (a, b),
        };
        if a > b {
            continue;
        }
        cols += (index(b, tier.lon0, tier.box_lon) - index(a, tier.lon0, tier.box_lon) + 1).max(0)
            as u64;
    }
    rows * cols.min(per_turn)
}

/// Blocks of hours of `block_hours` that `[first, last]` crosses.
fn time_blocks(first: i64, last: i64, block_hours: i64) -> u64 {
    if last < first {
        return 0;
    }
    let block = |t: i64| t.div_euclid(3600).div_euclid(block_hours);
    (block(last) - block(first) + 1) as u64
}

/// The part of the NW Shelf's box IBI's does not overlap, where IBI is
/// the first tier: south of 40N (the NW Shelf box holds IBI's longitudes).
const IBI_ONLY_BOX: ([f64; 2], [f64; 2]) = ([26.1, 39.999], [-19.1, 5.1]);

/// Expected download for a GRIB export over `area` at `times` (whole
/// hours, in order), computed from the area's extent and each dataset's
/// chunk and block geometry, so it costs the same for a bay or the globe
/// (spec.md 13):
///
/// - **ERA5**: for each hour, each variable's 64-byte head and the blocks
///   holding the area's rows (as [`estimate`]), less what `memory` holds.
/// - **Current**: one typical block per variable per chunk box and block
///   of hours, in the first tier whose box holds that part of the area:
///   the NW Shelf, IBI south of 40N, then the global merged current from
///   November 2020 or GlobCurrent before. Global boxes wholly inside a
///   regional box are not taken out (an estimate).
pub fn estimate_export(area: &Area, times: &[i64], memory: Option<&BlockCache>) -> ExportEstimate {
    use crate::dataset::HEAD_REQUEST;
    use crate::memory::Part;
    let mut out = ExportEstimate::default();
    let blocks = era5_area_blocks(area);
    let held = |dataset: Dataset, key: &str, part: Part| {
        memory.is_some_and(|m| m.contains(dataset.id(), key, part))
    };
    for &hour in times {
        let (wind_ds, wind_index) = if (WB2_FIRST..=WB2_LAST).contains(&hour) {
            (Dataset::Wb2Era5Hourly, (hour - WB2_FIRST) / 3600)
        } else {
            (Dataset::ArcoEra5, (hour - ARCO_FIRST) / 3600)
        };
        let arco_index = (hour - ARCO_FIRST) / 3600;
        let chunks = [
            (wind_ds, vars::WB2_U10.array, wind_index, &WIND_BLOCKS, true),
            (wind_ds, vars::WB2_V10.array, wind_index, &WIND_BLOCKS, true),
            (
                Dataset::ArcoEra5,
                vars::ARCO_SWH.array,
                arco_index,
                &SWH_BLOCKS,
                false,
            ),
            (
                Dataset::ArcoEra5,
                vars::ARCO_MWD.array,
                arco_index,
                &MWD_BLOCKS,
                false,
            ),
        ];
        for (dataset, array, index, sizes, wind) in chunks {
            let key = format!("{array}/{index}.0.0");
            let (total, cached) = if wind {
                (&mut out.wind_bytes, &mut out.wind_cached_bytes)
            } else {
                (&mut out.waves_bytes, &mut out.waves_cached_bytes)
            };
            let mut add = |bytes: u64, part: Part| {
                if held(dataset, &key, part) {
                    *cached += bytes;
                } else {
                    *total += bytes;
                }
            };
            add(HEAD_REQUEST, Part::Head);
            for &block in &blocks {
                add(
                    sizes.get(block).copied().unwrap_or_default(),
                    Part::Block(block as u32),
                );
            }
        }
    }
    let (Some(&first), Some(&last)) = (times.first(), times.last()) else {
        return out;
    };
    let cost = |tier: &CurrentTier, boxes: u64, from: i64, to: i64| {
        boxes
            * time_blocks(from, to, tier.block_hours)
            * tier.variables
            * (tier.block_bytes + HEAD_REQUEST)
    };
    let nws = tier_boxes(&NWS_TIER, area, Some(NWS_BOX));
    let ibi = tier_boxes(&IBI_TIER, area, Some(IBI_ONLY_BOX));
    // The global tiers, less the part of the area the regional ones take.
    let covered = |tier: &CurrentTier| {
        tier_boxes(tier, area, None).saturating_sub(
            tier_boxes(tier, &inner(area, NWS_BOX), None)
                + tier_boxes(tier, &inner(area, IBI_ONLY_BOX), None),
        )
    };
    out.current_bytes = cost(&NWS_TIER, nws, first, last)
        + cost(&IBI_TIER, ibi, first, last)
        + cost(
            &MERGED_TIER,
            covered(&MERGED_TIER),
            first.max(MERGED_FIRST),
            last,
        )
        + cost(
            &GLOBCURRENT_TIER,
            covered(&GLOBCURRENT_TIER),
            first,
            last.min(MERGED_FIRST - 1),
        );
    out
}

/// The global tier boxes wholly inside `region` are the ones a smaller
/// area shrunk by a box's size on each side would still touch; this is
/// that region's intersection with `area`, shrunk by 2° (larger than any
/// global box), or an empty area.
fn inner(area: &Area, (lat, lon): ([f64; 2], [f64; 2])) -> Area {
    let empty = Area {
        north: -91.0,
        south: 91.0,
        west: 0.0,
        width: -1.0,
    };
    let (south, north) = (area.south.max(lat[0] + 2.0), area.north.min(lat[1] - 2.0));
    if south > north {
        return empty;
    }
    // Only an area that does not wrap past the region is shrunk; others
    // keep the global boxes (an over-estimate).
    let segments = area.lon_segments();
    let [(a, b)] = segments.as_slice() else {
        return empty;
    };
    let (west, east) = (a.max(lon[0] + 2.0), b.min(lon[1] - 2.0));
    if west > east {
        return empty;
    }
    Area {
        north,
        south,
        west,
        width: east - west,
    }
}

/// The current part of [`estimate`].
fn current_bytes(points: &[Point]) -> u64 {
    let mut blocks: Vec<(u8, i64, i64, i64)> = points
        .iter()
        .map(|p| {
            let (which, tier) = if in_box(p, NWS_BOX) {
                (0, &NWS_TIER)
            } else if in_box(p, IBI_BOX) {
                (1, &IBI_TIER)
            } else if p.t >= MERGED_FIRST {
                (2, &MERGED_TIER)
            } else {
                (3, &GLOBCURRENT_TIER)
            };
            let lon = (p.lon + 180.0).rem_euclid(360.0) - 180.0;
            (
                which,
                ((p.lat - tier.lat0) / tier.box_lat).floor() as i64,
                ((lon - tier.lon0) / tier.box_lon).floor() as i64,
                p.t.div_euclid(3600).div_euclid(tier.block_hours),
            )
        })
        .collect();
    blocks.sort_unstable();
    blocks.dedup();
    blocks
        .iter()
        .map(|(which, ..)| {
            let tier = [&NWS_TIER, &IBI_TIER, &MERGED_TIER, &GLOBCURRENT_TIER][usize::from(*which)];
            tier.variables * (tier.block_bytes + crate::dataset::HEAD_REQUEST)
        })
        .sum()
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
    fn hourly_brackets_use_adjacent_hours() {
        assert_eq!(
            bracket_every(&axis(), 4 * 3600 + 1800, 3600),
            Some((4, 5, 0.5))
        );
        assert_eq!(bracket_every(&axis(), 6 * 3600, 3600), Some((6, 6, 0.0)));
        assert_eq!(
            bracket_every(&axis(), 46 * 3600 + 1800, 3600),
            Some((46, 47, 0.5))
        );
        assert_eq!(bracket_every(&axis(), -1, 3600), None);
    }

    #[test]
    fn intervals_round_trip_through_seconds() {
        let i = Interval::Hourly;
        assert_eq!(Interval::from_seconds(i.seconds()), Some(i));
        assert_eq!(Interval::from_seconds(7200), None);
        assert_eq!(Interval::from_seconds(10_800), None);
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

    /// Hand-counted: a 24-hour track at 50.1N 4.9W sampled every 10
    /// minutes needs 25 hours hourly (00Z to 24Z) at hourly sampling. Its
    /// stencil is rows 159–160, columns 1420–1421: values 230,380 to
    /// 231,821, all in block 1 (131,072 to 262,143). Each hour is five
    /// 64-byte heads and block 1 of u, v (432,731 each), wave height
    /// (173,816), direction (165,715) and period (168,068): 1,373,381 bytes. The current is
    /// one NW Shelf box and block of hours: 2 × (150,000 + 64).
    #[test]
    fn the_estimate_counts_heads_blocks_and_current_boxes() {
        let t0 = crate::time::parse_utc("2020-07-27T00:00Z").expect("a time");
        let points: Vec<Point> = (0..=144)
            .map(|k| Point {
                t: t0 + k * 600,
                lat: 50.1,
                lon: -4.9,
            })
            .collect();
        assert_eq!(era5_blocks(50.1, -4.9), vec![1]);
        let e = estimate(&points, None);
        let hour = 5 * 64 + 2 * 432_731 + 173_816 + 165_715 + 168_068;
        assert_eq!(hour, 1_373_381);
        let current = 2 * (150_000 + 64);
        assert_eq!(e.hourly_bytes, 25 * hour + current);
        assert_eq!(e.hourly_cached_bytes, 0);
    }

    /// The poles, the 0/360 seam and a stencil across a block boundary:
    /// row 182 runs from value 262,080 to 263,519, so columns before 64
    /// are in block 1 and the row below in block 2.
    #[test]
    fn era5_blocks_follow_the_rows_the_stencil_needs() {
        assert_eq!(era5_blocks(90.0, 0.0), vec![0]);
        assert_eq!(era5_blocks(-90.0, 10.0), vec![7]);
        // 359.9E wraps to column 0 on the same rows.
        assert_eq!(era5_blocks(0.0, 359.9), era5_blocks(0.0, 359.8));
        // 44.375N 13E: rows 182–183, columns 52–53.
        assert_eq!(era5_blocks(44.375, 13.0), vec![1, 2]);
    }

    /// Hand-counted: an export over one point, 50.1N 4.9W, for two
    /// WeatherBench2 hours: its stencil rows 159–160, columns 1420–1421,
    /// are values 230,380 to 231,821, block 1 of each chunk: per hour, u
    /// and v 2 × (64 + 432,731), waves 64 + 173,816 and 64 + 165,715. The
    /// current is one NW Shelf box and one block of hours (8,192), two
    /// variables.
    #[test]
    fn the_export_estimate_counts_blocks_per_hour_and_part() {
        let t = crate::time::parse_utc("2020-07-27T00:00Z").expect("a time");
        let area = Area {
            north: 50.1,
            south: 50.1,
            west: -4.9,
            width: 0.0,
        };
        assert_eq!(era5_area_blocks(&area), vec![1]);
        let e = estimate_export(&area, &[t, t + 3600], None);
        assert_eq!(e.wind_bytes, 2 * 2 * (64 + 432_731));
        assert_eq!(e.waves_bytes, 2 * (64 + 173_816 + 64 + 165_715));
        assert_eq!(e.current_bytes, 2 * (150_000 + 64));
        assert_eq!(e.wind_cached_bytes + e.waves_cached_bytes, 0);
        assert_eq!(estimate_export(&area, &[], None), ExportEstimate::default());
    }

    /// Chunk boxes are counted from the extents, across the antimeridian
    /// too: GlobCurrent's 2° × 1° boxes over 10S–9.99N (rows 40 to 49 of
    /// its grid from 90S: 10) and 175E–175W (10° across 180°: columns 355
    /// to 359 east of it and 0 to 5 west: 11).
    #[test]
    fn current_boxes_follow_the_area_across_the_antimeridian() {
        let area = Area {
            north: 9.99,
            south: -10.0,
            west: 175.0,
            width: 10.0,
        };
        assert_eq!(area.lon_segments().len(), 2);
        assert_eq!(tier_boxes(&GLOBCURRENT_TIER, &area, None), 10 * 11);
        // Outside the NW Shelf's box: none of its boxes.
        assert_eq!(tier_boxes(&NWS_TIER, &area, Some(NWS_BOX)), 0);
    }

    /// A circumnavigation's area for 90 days hourly is estimated from the
    /// extents in far less than the interaction budget (spec.md 13, 100
    /// ms), with nothing proportional to the area held: every block of
    /// every hour, and the current's global boxes.
    #[test]
    fn a_global_ninety_day_estimate_is_quick() {
        let t = crate::time::parse_utc("2024-11-10T00:00Z").expect("a time");
        let times: Vec<i64> = (0..90 * 24).map(|k| t + k * 3600).collect();
        let area = Area {
            north: 90.0,
            south: -90.0,
            west: 0.0,
            width: 360.0,
        };
        let start = std::time::Instant::now();
        let e = estimate_export(&area, &times, None);
        let took = start.elapsed();
        assert_eq!(era5_area_blocks(&area), (0..8).collect::<Vec<_>>());
        let wind_hour: u64 = 2 * (64 + WIND_BLOCKS.iter().sum::<u64>());
        assert_eq!(e.wind_bytes, 2160 * wind_hour);
        assert!(e.current_bytes > 0);
        assert!(took.as_millis() < 100, "{took:?}");
    }

    /// Blocks already downloaded this session are not counted again: the
    /// second boat of a race.
    #[test]
    fn blocks_in_memory_are_not_counted() {
        use crate::memory::{Held, Part};
        let memory = BlockCache::new(1 << 30);
        let t = crate::time::parse_utc("2020-07-27T12:00Z").expect("a time");
        // 539724: the WB2 index of that hour (time.rs test).
        let key = "10m_u_component_of_wind/539724.0.0";
        memory.put("wb2-era5-1h", key, Part::Head, Held::Missing);
        memory.put(
            "wb2-era5-1h",
            key,
            Part::Block(1),
            Held::Compressed(zarrs_storage::Bytes::from_static(b"x")),
        );
        let e = estimate(
            &[Point {
                t,
                lat: 50.0,
                lon: -5.0,
            }],
            Some(&memory),
        );
        assert_eq!(e.hourly_cached_bytes, 64 + 432_731);
        assert_eq!(
            e.hourly_bytes,
            4 * 64 + 432_731 + 173_816 + 165_715 + 168_068 + 2 * (150_000 + 64)
        );
    }
}
