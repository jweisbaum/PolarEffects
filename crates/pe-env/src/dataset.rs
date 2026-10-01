//! The datasets and variables PolarEffects reads (spec.md 7.5, 7.5.1), and
//! reading a gridded variable at a place and time.
//!
//! Every variable here is a Zarr v2 array `(time, [level,] latitude,
//! longitude)` compressed with blosc/LZ4. What differs is where the chunk
//! boundaries fall, which is what decides the cost of a track:
//!
//! - **WeatherBench2 and ARCO-ERA5** keep one *global* field per chunk: every
//!   hour of every variable is a separate 2–3.5 MB download, wherever the
//!   boat is.
//! - **Copernicus Marine geoChunked** keeps months of a small box of cells
//!   per chunk: a track costs about one chunk per box it crosses, whatever
//!   its length.
//!
//! Neither needs the whole chunk. A blosc chunk is a header, a table of
//! block offsets, and blocks that decode independently (an ERA5 block is
//! 131,072 values: about 91 rows of the globe; a geoChunk block about 1,000
//! to 8,000 hours of its box). [`OpenVariable::read_cells`] groups the cells
//! a batch of track positions needs by chunk, reads each chunk's first
//! bytes by HTTP range, then only the blocks holding those cells, adjacent
//! blocks in one request, on at most `concurrency` threads. What it
//! downloads is kept in the session's memory ([`crate::memory`]), never on
//! disk. A server that ignores ranges answers with the whole chunk, which is
//! then used whole.

use std::collections::BTreeMap;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use zarrs::array::codec::api::CodecOptions;
use zarrs::config::MetadataRetrieveVersion;
use zarrs::group::Group;
use zarrs::storage::ReadableStorage;
use zarrs_storage::Bytes;

use crate::blosc;
use crate::error::{EnvError, Result};
use crate::grid::{Axis, Grid, Stencil};
use crate::http::Ranged;
use crate::memory::{BlockCache, Held, Part};
use crate::store::{
    OpenStore, RangeSource, ReadArray, TimeAxis, dtype_name, open_array, read_axis, read_time_axis,
};

/// The WeatherBench2 hourly ERA5 store (D12). Frozen at 2023-01-10.
pub const WB2_HOURLY_URL: &str = "https://storage.googleapis.com/weatherbench2/datasets/era5/1959-2023_01_10-full_37-1h-0p25deg-chunk-1.zarr";

/// ARCO-ERA5, updated as ECMWF publishes. Zarr v2 despite the `-v3` in the
/// name.
pub const ARCO_ERA5_URL: &str = "https://storage.googleapis.com/gcp-public-data-arco-era5/ar/full_37-1h-0p25deg-chunk-1.zarr-v3";

/// The Copernicus Marine NW European Shelf reanalysis surface current,
/// hourly, about 7 km (spec.md 7.5.1 tier 1).
pub const CMEMS_NWS_MY_URL: &str = "https://s3.waw3-1.cloudferro.com/mdl-arco-geo-041/arco/NWSHELF_MULTIYEAR_PHY_004_009/cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i_202112/geoChunked.zarr";

/// The Copernicus Marine Iberia–Biscay–Ireland reanalysis current, hourly
/// means, 1/36° (spec.md 7.5.1 tier 1).
pub const CMEMS_IBI_MY_URL: &str = "https://s3.waw3-1.cloudferro.com/mdl-arco-geo-032/arco/IBI_MULTIYEAR_PHY_005_002/cmems_mod_ibi_phy-cur_my_0.027deg_PT1H-m_202511/geoChunked.zarr";

/// The Copernicus Marine global merged surface current, hourly, 1/12°,
/// chunked for time series at a place (spec.md 7.5.1 tier 2).
pub const CMEMS_MERGED_GEO_URL: &str = "https://s3.waw3-1.cloudferro.com/mdl-arco-geo-015/arco/GLOBAL_ANALYSISFORECAST_PHY_001_024/cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211/geoChunked.zarr";

/// GlobCurrent (MULTIOBS), multi-year, hourly, 0.25°, geostrophic + Ekman +
/// tide (FES2022) (spec.md 7.5.1 tier 3).
pub const GLOBCURRENT_MY_URL: &str = "https://s3.waw3-1.cloudferro.com/mdl-arco-geo-037/arco/MULTIOBS_GLO_PHY_MYNRT_015_003/cmems_obs-mob_glo_phy-cur_my_0.25deg_PT1H-i_202411/geoChunked.zarr";

/// GlobCurrent near real time, after the multi-year series ends.
pub const GLOBCURRENT_NRT_URL: &str = "https://s3.waw3-1.cloudferro.com/mdl-arco-geo-039/arco/MULTIOBS_GLO_PHY_MYNRT_015_003/cmems_obs-mob_glo_phy-cur_nrt_0.25deg_PT1H-i_202411/geoChunked.zarr";

/// A dataset: one Zarr store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dataset {
    /// WeatherBench2 ERA5, hourly, 0.25°, 1959 to 2023-01-10.
    Wb2Era5Hourly,
    /// ARCO-ERA5, hourly, 0.25°, 1940 to a few days ago.
    ArcoEra5,
    /// Copernicus Marine NW European Shelf reanalysis, 1993 on.
    CmemsNwsMy,
    /// Copernicus Marine Iberia–Biscay–Ireland reanalysis, 1993 on.
    CmemsIbiMy,
    /// Copernicus Marine global merged current, 2020-11 onward.
    CmemsGlobalMerged,
    /// GlobCurrent multi-year, 1993 on.
    GlobCurrentMy,
    /// GlobCurrent near real time.
    GlobCurrentNrt,
}

impl Dataset {
    /// Every dataset.
    pub const ALL: [Self; 7] = [
        Self::Wb2Era5Hourly,
        Self::ArcoEra5,
        Self::CmemsNwsMy,
        Self::CmemsIbiMy,
        Self::CmemsGlobalMerged,
        Self::GlobCurrentMy,
        Self::GlobCurrentNrt,
    ];

    /// The name recorded on each sample (`DatasetRecord::name`), and the
    /// chunk cache namespace.
    pub fn id(self) -> &'static str {
        match self {
            Self::Wb2Era5Hourly => "wb2-era5-1h",
            Self::ArcoEra5 => "arco-era5",
            Self::CmemsNwsMy => "cmems-nws-my-uv-geo",
            Self::CmemsIbiMy => "cmems-ibi-my-cur-geo",
            Self::CmemsGlobalMerged => "cmems-glo-merged-uv-geo",
            Self::GlobCurrentMy => "globcurrent-my-geo",
            Self::GlobCurrentNrt => "globcurrent-nrt-geo",
        }
    }

    /// Where it is published.
    pub fn url(self) -> &'static str {
        match self {
            Self::Wb2Era5Hourly => WB2_HOURLY_URL,
            Self::ArcoEra5 => ARCO_ERA5_URL,
            Self::CmemsNwsMy => CMEMS_NWS_MY_URL,
            Self::CmemsIbiMy => CMEMS_IBI_MY_URL,
            Self::CmemsGlobalMerged => CMEMS_MERGED_GEO_URL,
            Self::GlobCurrentMy => GLOBCURRENT_MY_URL,
            Self::GlobCurrentNrt => GLOBCURRENT_NRT_URL,
        }
    }

    /// The version recorded on each sample: the store's own dated name.
    pub fn version(self) -> &'static str {
        match self {
            Self::Wb2Era5Hourly => "1959-2023_01_10-full_37-1h-0p25deg-chunk-1",
            Self::ArcoEra5 => "full_37-1h-0p25deg-chunk-1.zarr-v3",
            Self::CmemsNwsMy => "cmems_mod_nws_phy-uv_my_7km-2D_PT1H-i_202112",
            Self::CmemsIbiMy => "cmems_mod_ibi_phy-cur_my_0.027deg_PT1H-m_202511",
            Self::CmemsGlobalMerged => "cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211",
            Self::GlobCurrentMy => "cmems_obs-mob_glo_phy-cur_my_0.25deg_PT1H-i_202411",
            Self::GlobCurrentNrt => "cmems_obs-mob_glo_phy-cur_nrt_0.25deg_PT1H-i_202411",
        }
    }

    /// Whether this dataset's current includes tides: `None` for wind and
    /// wave datasets. Every current tier read today does: the regional
    /// reanalyses are tidally forced, the merged current is read as uo +
    /// utide, and GlobCurrent 202411's `uo` is, by its own metadata,
    /// "absolute geostrophic velocity + depth Ekman + tide velocity"
    /// (FES2022; both the multi-year and near-real-time stores, checked
    /// 2026-09-28, plan.md Q7). The flag stays per dataset so a tier
    /// without tides can be marked, and filtered, should one be added.
    pub fn has_tide(self) -> Option<bool> {
        match self {
            Self::Wb2Era5Hourly | Self::ArcoEra5 => None,
            Self::CmemsNwsMy
            | Self::CmemsIbiMy
            | Self::CmemsGlobalMerged
            | Self::GlobCurrentMy
            | Self::GlobCurrentNrt => Some(true),
        }
    }

    /// The dataset whose [`Self::id`] this is.
    pub fn from_id(id: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|d| d.id() == id)
    }
}

/// Which way a direction points (CLAUDE.md conventions).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Sense {
    /// Not a direction.
    None,
    /// Meteorological: where it comes from (wind, waves).
    From,
    /// Oceanographic: where it goes (currents).
    Toward,
}

/// One variable PolarEffects samples.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Variable {
    /// The dataset it is read from.
    pub dataset: Dataset,
    /// The array name in the store.
    pub array: &'static str,
    /// Units as stored; converted on ingest (m/s → kn for wind).
    pub units: &'static str,
    /// Direction sense of the vector or angle this variable is part of.
    pub sense: Sense,
    /// Whether land is NaN (or fill) rather than a value: waves and currents
    /// have no value over land, wind does.
    pub land_is_missing: bool,
}

/// The variable table (spec.md 7.5, 7.5.1; CLAUDE.md "Adding a reanalysis
/// variable"). Integer-packed arrays (`int16` with `scale_factor`) are
/// unpacked on read from their own attributes, so the table does not repeat
/// the factors.
pub mod vars {
    use super::{Dataset, Sense, Variable};

    const fn v(
        dataset: Dataset,
        array: &'static str,
        units: &'static str,
        sense: Sense,
        land_is_missing: bool,
    ) -> Variable {
        Variable {
            dataset,
            array,
            units,
            sense,
            land_is_missing,
        }
    }

    const fn current(dataset: Dataset, array: &'static str) -> Variable {
        v(dataset, array, "m s-1", Sense::Toward, true)
    }

    /// 10 m wind, eastward, WeatherBench2.
    pub const WB2_U10: Variable = v(
        Dataset::Wb2Era5Hourly,
        "10m_u_component_of_wind",
        "m s-1",
        Sense::From,
        false,
    );
    /// 10 m wind, northward, WeatherBench2.
    pub const WB2_V10: Variable = v(
        Dataset::Wb2Era5Hourly,
        "10m_v_component_of_wind",
        "m s-1",
        Sense::From,
        false,
    );
    /// 10 m wind, eastward, ARCO-ERA5 (after WeatherBench2 ends, D12).
    pub const ARCO_U10: Variable = v(
        Dataset::ArcoEra5,
        "10m_u_component_of_wind",
        "m s-1",
        Sense::From,
        false,
    );
    /// 10 m wind, northward, ARCO-ERA5.
    pub const ARCO_V10: Variable = v(
        Dataset::ArcoEra5,
        "10m_v_component_of_wind",
        "m s-1",
        Sense::From,
        false,
    );
    /// Significant height of combined wind waves and swell.
    pub const ARCO_SWH: Variable = v(
        Dataset::ArcoEra5,
        "significant_height_of_combined_wind_waves_and_swell",
        "m",
        Sense::None,
        true,
    );
    /// Mean wave direction, degrees, "from".
    pub const ARCO_MWD: Variable = v(
        Dataset::ArcoEra5,
        "mean_wave_direction",
        "degree true",
        Sense::From,
        true,
    );
    /// Mean period of combined wind waves and swell, seconds.
    pub const ARCO_MWP: Variable = v(
        Dataset::ArcoEra5,
        "mean_wave_period",
        "s",
        Sense::None,
        true,
    );
    /// NW Shelf total current, eastward (int16, scale 0.001).
    pub const NWS_UO: Variable = current(Dataset::CmemsNwsMy, "uo");
    /// NW Shelf total current, northward.
    pub const NWS_VO: Variable = current(Dataset::CmemsNwsMy, "vo");
    /// IBI total current, eastward.
    pub const IBI_UO: Variable = current(Dataset::CmemsIbiMy, "uo");
    /// IBI total current, northward.
    pub const IBI_VO: Variable = current(Dataset::CmemsIbiMy, "vo");
    /// Eulerian (circulation) current, eastward.
    pub const CMEMS_UO: Variable = current(Dataset::CmemsGlobalMerged, "uo");
    /// Eulerian (circulation) current, northward.
    pub const CMEMS_VO: Variable = current(Dataset::CmemsGlobalMerged, "vo");
    /// Tidal current, eastward.
    pub const CMEMS_UTIDE: Variable = current(Dataset::CmemsGlobalMerged, "utide");
    /// Tidal current, northward.
    pub const CMEMS_VTIDE: Variable = current(Dataset::CmemsGlobalMerged, "vtide");
    /// Stokes drift, eastward.
    pub const CMEMS_VSDX: Variable = current(Dataset::CmemsGlobalMerged, "vsdx");
    /// Stokes drift, northward.
    pub const CMEMS_VSDY: Variable = current(Dataset::CmemsGlobalMerged, "vsdy");
    /// Total current including Stokes drift, eastward.
    pub const CMEMS_UTOTAL: Variable = current(Dataset::CmemsGlobalMerged, "utotal");
    /// Total current including Stokes drift, northward.
    pub const CMEMS_VTOTAL: Variable = current(Dataset::CmemsGlobalMerged, "vtotal");
    /// GlobCurrent multi-year current, eastward (int16, scale 0.001).
    pub const GC_MY_UO: Variable = current(Dataset::GlobCurrentMy, "uo");
    /// GlobCurrent multi-year current, northward.
    pub const GC_MY_VO: Variable = current(Dataset::GlobCurrentMy, "vo");
    /// GlobCurrent near-real-time current, eastward.
    pub const GC_NRT_UO: Variable = current(Dataset::GlobCurrentNrt, "uo");
    /// GlobCurrent near-real-time current, northward.
    pub const GC_NRT_VO: Variable = current(Dataset::GlobCurrentNrt, "vo");
}

/// Values at or above this magnitude are fill, not data. Copernicus Marine
/// writes 9.969e36 (the netCDF default fill) for land and missing chunks.
const FILL_MAGNITUDE: f32 = 1e30;

/// How far an archive's data runs, from ARCO-ERA5's root attributes.
///
/// ARCO-ERA5's time axis is preallocated far past today; the axis is no
/// evidence that a step exists (CLAUDE.md "Environment gotchas").
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Coverage {
    /// First written time, UTC epoch seconds.
    pub first: Option<i64>,
    /// Last written time, final stream included, UTC epoch seconds.
    pub last: Option<i64>,
}

/// Reads `valid_time_start` / `valid_time_stop(_era5t)` off the root group.
///
/// # Errors
/// [`EnvError::Open`] if the root group cannot be read.
pub fn read_coverage(store: &ReadableStorage) -> Result<Coverage> {
    let group = Group::open_opt(store.clone(), "/", &MetadataRetrieveVersion::V2)
        .map_err(|e| EnvError::Open(format!("the root group: {e}")))?;
    let attrs = group.attributes();
    let day = |key: &str, end_of_day: bool| -> Option<i64> {
        let text = attrs.get(key)?.as_str()?;
        let start = crate::time::parse_utc(text)?;
        Some(if end_of_day { start + 23 * 3600 } else { start })
    };
    Ok(Coverage {
        first: day("valid_time_start", false),
        last: day("valid_time_stop_era5t", true).or_else(|| day("valid_time_stop", true)),
    })
}

/// How the stored numbers become values.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Stored {
    /// `float32`; NaN and ≥ 1e30 are fill.
    F32,
    /// `float64`; as `F32`.
    F64,
    /// `int16` packed with `scale_factor` / `add_offset`; `fill` is missing.
    I16 { fill: i16 },
}

/// Unpacking for one array.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Unpack {
    stored: Stored,
    scale: f64,
    offset: f64,
}

impl Unpack {
    fn float(&self, v: f64) -> f32 {
        if !v.is_finite() || v.abs() >= f64::from(FILL_MAGNITUDE) {
            f32::NAN
        } else {
            (v * self.scale + self.offset) as f32
        }
    }

    fn int(&self, v: i16, fill: i16) -> f32 {
        if v == fill {
            f32::NAN
        } else {
            (f64::from(v) * self.scale + self.offset) as f32
        }
    }
}

/// One grid cell of a variable at one time step: `(step, lat index, lon
/// index)`.
pub type Cell = (u64, usize, usize);

/// The cells wanted from one chunk: its index, its store key, and each
/// cell with its offset inside it.
type ChunkRequest = (Vec<u64>, String, Vec<(Cell, usize)>);

/// How many bytes of a chunk are asked for first: its blosc header and, for
/// up to 12 blocks, their offsets (an ERA5 chunk has 8, a geoChunk 5 to 7).
/// A chunk with more blocks costs a second, exact request.
pub const HEAD_REQUEST: u64 = 64;

/// How a chunk's bytes become values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Layout {
    /// A blosc container of little-endian values in C order with no
    /// filters, every archive here: read one block at a time.
    Blocks,
    /// Anything else (the uncompressed stores tests write): whole chunks,
    /// through `zarrs`.
    Whole,
}

/// Decides the [`Layout`] from the array's `.zarray`.
fn layout_of(store: &ReadableStorage, array: &str) -> Layout {
    let key = format!("{array}/.zarray");
    let meta: Option<serde_json::Value> = zarrs::storage::StoreKey::new(key)
        .ok()
        .and_then(|k| store.get(&k).ok().flatten())
        .and_then(|bytes| serde_json::from_slice(&bytes).ok());
    let Some(meta) = meta else {
        return Layout::Whole;
    };
    let blosc = meta["compressor"]["id"].as_str() == Some("blosc");
    let no_filters =
        meta["filters"].is_null() || meta["filters"].as_array().is_some_and(|f| f.is_empty());
    let c_order = meta["order"].as_str().is_none_or(|o| o == "C");
    let little = meta["dtype"]
        .as_str()
        .is_some_and(|d| d.starts_with('<') || d.starts_with('|'));
    if blosc && no_filters && c_order && little {
        Layout::Blocks
    } else {
        Layout::Whole
    }
}

/// An opened variable: its array, grid and time axis.
pub struct OpenVariable {
    spec: Variable,
    array: ReadArray,
    grid: Grid,
    time: TimeAxis,
    /// Index read along each dimension between time and latitude (a level):
    /// the one nearest the surface.
    middle: Vec<u64>,
    unpack: Unpack,
    concurrency: usize,
    layout: Layout,
    /// The store's objects by byte range.
    ranges: Arc<dyn RangeSource>,
    /// What this session has downloaded, shared with every other variable.
    memory: Arc<BlockCache>,
    /// Set once the store has answered a range with the whole object: it
    /// ignores ranges, so every later chunk is read whole at once.
    whole_only: AtomicBool,
}

impl std::fmt::Debug for OpenVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenVariable")
            .field("spec", &self.spec)
            .field("grid", &self.grid)
            .field("time", &self.time)
            .field("layout", &self.layout)
            .finish_non_exhaustive()
    }
}

/// A number attribute of an array.
fn number_attr(array: &ReadArray, key: &str) -> Option<f64> {
    array
        .attributes()
        .get(key)
        .and_then(serde_json::Value::as_f64)
}

impl OpenVariable {
    /// Opens `spec`'s array on `store` and reads its axes.
    ///
    /// # Errors
    /// [`EnvError::Layout`] if the array is not `(time, …, latitude,
    /// longitude)` on regular axes, stored as float32, float64 or int16.
    pub fn open(open: &OpenStore, spec: Variable) -> Result<Self> {
        let store = &open.store;
        let array = open_array(store, &format!("/{}", spec.array))?;
        let dims: Vec<String> = array
            .attributes()
            .get("_ARRAY_DIMENSIONS")
            .and_then(|v| v.as_array())
            .map(|a| {
                a.iter()
                    .filter_map(|d| d.as_str().map(str::to_owned))
                    .collect()
            })
            .unwrap_or_default();
        let n = array.shape().len();
        if n < 3
            || dims.len() != n
            || dims[0] != "time"
            || dims[n - 2] != "latitude"
            || dims[n - 1] != "longitude"
        {
            return Err(EnvError::Layout(format!(
                "{} has dimensions {dims:?}; expected (time, …, latitude, longitude)",
                spec.array
            )));
        }
        let dtype = dtype_name(&array);
        let stored = if dtype.contains("float32") {
            Stored::F32
        } else if dtype.contains("float64") {
            Stored::F64
        } else if dtype.contains("int16") && !dtype.contains("uint16") {
            let bytes = array.fill_value().as_ne_bytes();
            let fill = <[u8; 2]>::try_from(bytes)
                .map(i16::from_ne_bytes)
                .map_err(|_| EnvError::Layout(format!("{} has no int16 fill value", spec.array)))?;
            Stored::I16 { fill }
        } else {
            return Err(EnvError::Layout(format!(
                "{} is stored as {dtype}; this reader expects float32, float64 or int16",
                spec.array
            )));
        };
        let unpack = Unpack {
            stored,
            scale: number_attr(&array, "scale_factor").unwrap_or(1.0),
            offset: number_attr(&array, "add_offset").unwrap_or(0.0),
        };
        let lat = read_axis(store, "/latitude", "the latitude axis")?;
        let lon = read_axis(store, "/longitude", "the longitude axis")?;
        let grid = Grid {
            lat: Axis::from_values("latitude", &lat)?,
            lon: Axis::from_values("longitude", &lon)?,
        };
        let shape = array.shape().to_vec();
        if shape[n - 2] != grid.lat.len as u64 || shape[n - 1] != grid.lon.len as u64 {
            return Err(EnvError::Layout(format!(
                "{} is {:?} but the axes are {} x {}",
                spec.array, shape, grid.lat.len, grid.lon.len
            )));
        }
        let mut time = read_time_axis(store, "/time")?;
        if time.len != shape[0] {
            return Err(EnvError::Layout(format!(
                "{} has {} steps but the time axis {}",
                spec.array, shape[0], time.len
            )));
        }
        // The data can stop before a preallocated axis does.
        if spec.dataset == Dataset::ArcoEra5
            && let Some(last) = read_coverage(store)?.last
            && last < time.last()
        {
            time.len = ((last - time.first) / time.step + 1).max(0) as u64;
        }
        // A level between time and latitude (GlobCurrent keeps 0 m and
        // 15 m): read the one nearest the surface, by its coordinate when
        // the store has one.
        let middle = dims[1..n - 2]
            .iter()
            .zip(&shape[1..n - 2])
            .map(|(name, len)| {
                read_axis(store, &format!("/{name}"), "a level axis")
                    .ok()
                    .filter(|values| values.len() as u64 == *len)
                    .and_then(|values| {
                        values
                            .iter()
                            .enumerate()
                            .min_by(|a, b| a.1.abs().total_cmp(&b.1.abs()))
                            .map(|(i, _)| i as u64)
                    })
                    .unwrap_or(0)
            })
            .collect();
        Ok(Self {
            layout: layout_of(store, spec.array),
            spec,
            array,
            grid,
            time,
            middle,
            unpack,
            concurrency: 8,
            ranges: Arc::clone(&open.ranges),
            memory: BlockCache::new(crate::memory::DEFAULT_LIMIT),
            whole_only: AtomicBool::new(false),
        })
    }

    /// Limits how many chunks are fetched and decoded at once (spec.md 3.4).
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.max(1);
        self
    }

    /// Keeps what this variable downloads in `memory`, shared with the
    /// session's other reads, instead of its own.
    pub fn with_memory(mut self, memory: Arc<BlockCache>) -> Self {
        self.memory = memory;
        self
    }

    /// The variable this was opened for.
    pub fn spec(&self) -> Variable {
        self.spec
    }

    /// The grid.
    pub fn grid(&self) -> Grid {
        self.grid
    }

    /// The time axis, clamped to written data.
    pub fn time(&self) -> TimeAxis {
        self.time
    }

    /// Whether chunks are read a block at a time (blosc) rather than whole.
    pub fn reads_blocks(&self) -> bool {
        self.layout == Layout::Blocks
    }

    /// The chunk shape, `(time, …, latitude, longitude)`.
    ///
    /// # Errors
    /// [`EnvError::Layout`] if the chunk grid is not regular.
    pub fn chunk_shape(&self) -> Result<Vec<u64>> {
        let zeros = vec![0; self.array.shape().len()];
        self.array
            .chunk_shape(&zeros)
            .map(|shape| shape.iter().map(|n| n.get()).collect())
            .map_err(|e| EnvError::Layout(format!("{}: {e}", self.spec.array)))
    }

    /// The level index read along each middle dimension.
    pub fn levels(&self) -> &[u64] {
        &self.middle
    }

    /// The store key of the chunk at `index`, as the memory and the
    /// estimate name it.
    pub fn chunk_key(&self, index: &[u64]) -> String {
        self.array.chunk_key(index).as_str().to_owned()
    }

    /// The chunk that holds `cell`, and the cell's offset inside it.
    fn locate(&self, chunk: &[u64], cell: Cell) -> (Vec<u64>, usize) {
        let (step, lat, lon) = cell;
        let mut index: Vec<u64> = Vec::with_capacity(chunk.len());
        index.push(step);
        index.extend(self.middle.iter().copied());
        index.push(lat as u64);
        index.push(lon as u64);
        let mut chunk_index = Vec::with_capacity(chunk.len());
        let mut offset: u64 = 0;
        for (i, size) in index.iter().zip(chunk) {
            chunk_index.push(i / size);
            offset = offset * size + i % size;
        }
        (chunk_index, offset as usize)
    }

    fn namespace(&self) -> &'static str {
        self.spec.dataset.id()
    }

    /// Bytes per stored value.
    fn typesize(&self) -> usize {
        match self.unpack.stored {
            Stored::F32 => 4,
            Stored::F64 => 8,
            Stored::I16 { .. } => 2,
        }
    }

    /// The value stored at `at` in decoded little-endian bytes.
    fn value_at(&self, bytes: &[u8], at: usize) -> f32 {
        let ts = self.typesize();
        let Some(raw) = bytes.get(at..at + ts) else {
            return f32::NAN;
        };
        match self.unpack.stored {
            Stored::F32 => raw.try_into().map_or(f32::NAN, |b| {
                self.unpack.float(f64::from(f32::from_le_bytes(b)))
            }),
            Stored::F64 => raw
                .try_into()
                .map_or(f32::NAN, |b| self.unpack.float(f64::from_le_bytes(b))),
            Stored::I16 { fill } => raw
                .try_into()
                .map_or(f32::NAN, |b| self.unpack.int(i16::from_le_bytes(b), fill)),
        }
    }

    /// Decodes a whole chunk through `zarrs` (a store that is not blosc).
    fn decode_whole(&self, index: &[u64]) -> Result<Option<Arc<Vec<f32>>>> {
        let what = self.spec.array;
        let err = |e: zarrs::array::ArrayError| EnvError::Read {
            what: format!("{what} chunk {index:?}"),
            source: Box::new(e),
        };
        let options = CodecOptions::default();
        let unpack = self.unpack;
        let values: Option<Vec<f32>> = match unpack.stored {
            Stored::F32 => self
                .array
                .retrieve_chunk_if_exists_opt::<Vec<f32>>(index, &options)
                .map_err(err)?
                .map(|v| v.into_iter().map(|x| unpack.float(f64::from(x))).collect()),
            Stored::F64 => self
                .array
                .retrieve_chunk_if_exists_opt::<Vec<f64>>(index, &options)
                .map_err(err)?
                .map(|v| v.into_iter().map(|x| unpack.float(x)).collect()),
            Stored::I16 { fill } => self
                .array
                .retrieve_chunk_if_exists_opt::<Vec<i16>>(index, &options)
                .map_err(err)?
                .map(|v| v.into_iter().map(|x| unpack.int(x, fill)).collect()),
        };
        Ok(values.map(Arc::new))
    }

    /// The values at `offsets` of a chunk read whole through `zarrs`.
    fn whole_values(&self, key: &str, index: &[u64], offsets: &[usize]) -> Result<Vec<f32>> {
        let ns = self.namespace();
        let values = match self.memory.get(ns, key, Part::Whole) {
            Some(Held::Values(values)) => Some(values),
            Some(_) => None,
            None => {
                let decoded = self.decode_whole(index)?;
                let held = decoded.clone().map_or(Held::Missing, Held::Values);
                self.memory.put(ns, key, Part::Whole, held);
                decoded
            }
        };
        Ok(offsets
            .iter()
            .map(|&o| {
                values
                    .as_ref()
                    .and_then(|v| v.get(o).copied())
                    .unwrap_or(f32::NAN)
            })
            .collect())
    }

    /// Keeps a whole blosc chunk's header and every block, as served, and
    /// returns them: the caller uses the blocks it was given, never reads
    /// them back through the size-bounded memory, which may already have
    /// let them go.
    fn keep_whole(&self, key: &str, bytes: &Bytes) -> Result<(Held, Vec<Bytes>)> {
        let expected = blosc::Expected::Exactly(self.chunk_bytes()?);
        let header = blosc::Header::parse(bytes, expected)?;
        if header.cbytes != bytes.len() {
            return Err(EnvError::Blosc(format!(
                "{key}: header declares {} bytes but the chunk is {}",
                header.cbytes,
                bytes.len()
            )));
        }
        let extents = header.block_extents(bytes)?;
        // Every block is checked before any is kept, so damaged bytes fail
        // this read and the next one asks the archive again. Each is copied
        // out, so a kept block does not hold the whole chunk alive beyond
        // what the memory counts.
        let mut blocks = Vec::with_capacity(extents.len());
        for (b, extent) in extents.iter().enumerate() {
            let block = bytes
                .get(extent.clone())
                .ok_or_else(|| EnvError::Blosc(format!("{key}: block {b} runs past the chunk")))?;
            header.decode_block(b, block)?;
            blocks.push(Bytes::copy_from_slice(block));
        }
        let ns = self.namespace();
        for (b, block) in blocks.iter().enumerate() {
            self.memory.put(
                ns,
                key,
                Part::Block(b as u32),
                Held::Compressed(block.clone()),
            );
        }
        let head = Held::Head {
            header,
            extents: Arc::new(extents),
        };
        self.memory.put(ns, key, Part::Head, head.clone());
        Ok((head, blocks))
    }

    /// Decoded bytes in one whole chunk.
    fn chunk_bytes(&self) -> Result<usize> {
        let elements: u64 = self.chunk_shape()?.iter().product();
        usize::try_from(elements)
            .ok()
            .and_then(|n| n.checked_mul(self.typesize()))
            .filter(|&n| n <= blosc::MAX_UNKNOWN_SIZE)
            .ok_or_else(|| EnvError::Layout(format!("{}: the chunk is too large", self.spec.array)))
    }

    /// A chunk's header and block offsets: from memory, or its first bytes
    /// by range (a second request when it has many blocks).
    fn head(&self, key: &str) -> Result<Held> {
        let ns = self.namespace();
        if let Some(held) = self.memory.get(ns, key, Part::Head) {
            return Ok(held);
        }
        let missing = || {
            self.memory.put(ns, key, Part::Head, Held::Missing);
            Ok(Held::Missing)
        };
        if self.whole_only.load(Ordering::Relaxed) {
            return match self.ranges.whole(key)? {
                Some(bytes) => Ok(self.keep_whole(key, &bytes)?.0),
                None => missing(),
            };
        }
        let expected = blosc::Expected::Exactly(self.chunk_bytes()?);
        let mut first = match self.ranges.range(key, 0, HEAD_REQUEST)? {
            None => return missing(),
            Some(Ranged::Whole(bytes)) => {
                self.whole_only.store(true, Ordering::Relaxed);
                return Ok(self.keep_whole(key, &bytes)?.0);
            }
            Some(Ranged::Part(bytes)) => bytes,
        };
        let header = blosc::Header::parse(&first, expected)?;
        if first.len() < header.index_len() {
            first = match self.ranges.range(key, 0, header.index_len() as u64)? {
                None => return missing(),
                Some(Ranged::Whole(bytes)) => {
                    self.whole_only.store(true, Ordering::Relaxed);
                    return Ok(self.keep_whole(key, &bytes)?.0);
                }
                Some(Ranged::Part(bytes)) => bytes,
            };
        }
        let head = Held::Head {
            header,
            extents: Arc::new(header.block_extents(&first)?),
        };
        self.memory.put(ns, key, Part::Head, head.clone());
        Ok(head)
    }

    /// The values at `offsets` of a blosc chunk, fetching only the blocks
    /// that hold them.
    fn block_values(&self, key: &str, offsets: &[usize], cancel: &AtomicBool) -> Result<Vec<f32>> {
        let Held::Head { header, extents } = self.head(key)? else {
            return Ok(vec![f32::NAN; offsets.len()]);
        };
        let ts = self.typesize();
        let span = header.block_span(0).len().max(1);
        if !span.is_multiple_of(ts) {
            return Err(EnvError::Blosc(format!(
                "{key}: {span}-byte blocks split {ts}-byte values"
            )));
        }
        let block_of = |offset: usize| offset * ts / span;
        let wanted: std::collections::BTreeSet<usize> =
            offsets.iter().map(|&o| block_of(o)).collect();
        if let Some(&last) = wanted.last()
            && last >= extents.len()
        {
            return Err(EnvError::Blosc(format!(
                "{key}: a value lies in block {last} of {}",
                extents.len()
            )));
        }
        // Blocks already downloaded this session save a download; the rest
        // are fetched, and every value is read from bytes held here, so a
        // block the memory lets go meanwhile is never read as missing.
        let ns = self.namespace();
        let mut got: BTreeMap<usize, Bytes> = BTreeMap::new();
        for &b in &wanted {
            if let Some(Held::Compressed(bytes)) = self.memory.get(ns, key, Part::Block(b as u32)) {
                got.insert(b, bytes);
            }
        }
        let absent: std::collections::BTreeSet<usize> = wanted
            .iter()
            .copied()
            .filter(|b| !got.contains_key(b))
            .collect();
        got.extend(self.fetch_blocks(key, &header, &extents, &absent, cancel)?);
        let mut decoded: BTreeMap<usize, Vec<u8>> = BTreeMap::new();
        for (b, bytes) in &got {
            decoded.insert(*b, header.decode_block(*b, bytes)?);
        }
        Ok(offsets
            .iter()
            .map(|&o| {
                let b = block_of(o);
                let start = header.block_span(b).start;
                decoded
                    .get(&b)
                    .map_or(f32::NAN, |bytes| self.value_at(bytes, o * ts - start))
            })
            .collect())
    }

    /// Downloads the blocks of `absent`, adjacent ones in one request, and
    /// returns their compressed bytes (also kept in memory for later).
    ///
    /// # Errors
    /// A chunk whose header was read but whose blocks the archive no longer
    /// has fails the read (its header is forgotten), rather than being
    /// taken as "no data": a sample marked fetched with nothing would never
    /// be asked for again, while a failed batch is resumed by Fetch weather….
    fn fetch_blocks(
        &self,
        key: &str,
        header: &blosc::Header,
        extents: &[std::ops::Range<usize>],
        absent: &std::collections::BTreeSet<usize>,
        cancel: &AtomicBool,
    ) -> Result<BTreeMap<usize, Bytes>> {
        let ns = self.namespace();
        let mut out = BTreeMap::new();
        // Runs of blocks that lie back to back in the chunk.
        let mut runs: Vec<Vec<usize>> = Vec::new();
        for &b in absent {
            match runs.last_mut() {
                Some(run)
                    if run
                        .last()
                        .is_some_and(|&p| extents[p].end == extents[b].start) =>
                {
                    run.push(b);
                }
                _ => runs.push(vec![b]),
            }
        }
        for run in runs {
            if cancel.load(Ordering::SeqCst) {
                return Err(EnvError::Cancelled);
            }
            let (Some(&first), Some(&last)) = (run.first(), run.last()) else {
                continue;
            };
            let (start, end) = (extents[first].start, extents[last].end);
            let bytes = match self.ranges.range(key, start as u64, end as u64)? {
                None => {
                    self.memory.remove(ns, key, Part::Head);
                    return Err(EnvError::Read {
                        what: key.to_owned(),
                        source: "the archive no longer has this chunk, whose header it just served"
                            .into(),
                    });
                }
                Some(Ranged::Whole(bytes)) => {
                    self.whole_only.store(true, Ordering::Relaxed);
                    let (_, blocks) = self.keep_whole(key, &bytes)?;
                    for (b, block) in blocks.into_iter().enumerate() {
                        if absent.contains(&b) {
                            out.insert(b, block);
                        }
                    }
                    return Ok(out);
                }
                Some(Ranged::Part(bytes)) => bytes,
            };
            if bytes.len() != end - start {
                return Err(EnvError::Blosc(format!(
                    "{key}: asked for {} bytes of blocks {first}–{last}, got {}",
                    end - start,
                    bytes.len()
                )));
            }
            for b in run {
                let extent = extents[b].start - start..extents[b].end - start;
                // Checked now, so a damaged block fails the read that got it
                // rather than being kept. Copied out, so the memory counts
                // what it holds.
                header.decode_block(b, &bytes[extent.clone()])?;
                let block = Bytes::copy_from_slice(&bytes[extent]);
                self.memory.put(
                    ns,
                    key,
                    Part::Block(b as u32),
                    Held::Compressed(block.clone()),
                );
                out.insert(b, block);
            }
        }
        Ok(out)
    }

    /// The values of `cells`, NaN where missing (land, fill, a chunk the
    /// archive does not have, or outside the array). Each chunk is read once,
    /// only the blocks holding the cells, on at most `concurrency` threads,
    /// and `cancel` is checked before each.
    ///
    /// # Errors
    /// [`EnvError::Cancelled`] once `cancel` is set; [`EnvError::Read`] on a
    /// failed read.
    pub fn read_cells(&self, cells: &[Cell], cancel: &AtomicBool) -> Result<BTreeMap<Cell, f32>> {
        self.read_cells_with(cells, cancel, self.concurrency)
    }

    fn read_cells_with(
        &self,
        cells: &[Cell],
        cancel: &AtomicBool,
        concurrency: usize,
    ) -> Result<BTreeMap<Cell, f32>> {
        let (mut out, groups) = self.group(cells)?;
        let read = crate::parallel::map_bounded(&groups, concurrency.max(1), |group| {
            self.read_group(group, cancel)
        })?;
        out.extend(read.into_iter().flatten());
        Ok(out)
    }

    /// `cells` grouped by the chunk that holds them, and NaN for those
    /// outside the array.
    fn group(&self, cells: &[Cell]) -> Result<(BTreeMap<Cell, f32>, Vec<ChunkRequest>)> {
        let chunk = self.chunk_shape()?;
        let shape = self.array.shape().to_vec();
        let mut by_chunk: BTreeMap<Vec<u64>, Vec<(Cell, usize)>> = BTreeMap::new();
        let mut outside = BTreeMap::new();
        for &cell in cells {
            let inside = cell.0 < shape[0]
                && (cell.1 as u64) < shape[shape.len() - 2]
                && (cell.2 as u64) < shape[shape.len() - 1];
            if !inside {
                outside.insert(cell, f32::NAN);
                continue;
            }
            let (index, offset) = self.locate(&chunk, cell);
            by_chunk.entry(index).or_default().push((cell, offset));
        }
        let groups = by_chunk
            .into_iter()
            .map(|(index, wanted)| {
                let key = self.chunk_key(&index);
                (index, key, wanted)
            })
            .collect();
        Ok((outside, groups))
    }

    /// The values of one chunk's cells.
    fn read_group(&self, group: &ChunkRequest, cancel: &AtomicBool) -> Result<Vec<(Cell, f32)>> {
        let (index, key, wanted) = group;
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        let offsets: Vec<usize> = wanted.iter().map(|(_, o)| *o).collect();
        let values = match self.layout {
            Layout::Blocks => self.block_values(key, &offsets, cancel)?,
            Layout::Whole => self.whole_values(key, index, &offsets)?,
        };
        Ok(wanted
            .iter()
            .zip(values)
            .map(|((cell, _), value)| (*cell, value))
            .collect())
    }

    /// The stencil values for time steps `steps` at `stencil`, one
    /// `[f32; 4]` per step, NaN where missing.
    ///
    /// # Errors
    /// [`EnvError::Read`] on a failed read.
    pub fn stencil_series(&self, steps: Range<u64>, stencil: &Stencil) -> Result<Vec<[f32; 4]>> {
        let cells: Vec<Cell> = steps
            .clone()
            .flat_map(|step| corner_cells(step, stencil))
            .collect();
        let values = self.read_cells(&cells, &AtomicBool::new(false))?;
        Ok(steps
            .map(|step| corners_of(&values, step, stencil))
            .collect())
    }

    /// [`Self::stencil_series`] for scattered steps: the shape of a track,
    /// whose hours are known up front and each need their own global chunk.
    ///
    /// # Errors
    /// The first failed read.
    pub fn stencil_at_steps(
        &self,
        steps: &[u64],
        stencil: &Stencil,
        concurrency: usize,
    ) -> Result<Vec<[f32; 4]>> {
        let cells: Vec<Cell> = steps
            .iter()
            .flat_map(|&step| corner_cells(step, stencil))
            .collect();
        let values = self.read_cells_with(&cells, &AtomicBool::new(false), concurrency)?;
        Ok(steps
            .iter()
            .map(|&step| corners_of(&values, step, stencil))
            .collect())
    }

    /// The value at `(lat, lon)` and UTC epoch second `t`: bilinear in
    /// space, linear in time (spec.md 7.5). `Ok(None)` outside the grid or
    /// the data, or where every corner is missing.
    ///
    /// Directions are not interpolated here: a caller interpolates the
    /// components (u, v) or unit vector and takes the angle after.
    ///
    /// # Errors
    /// [`EnvError::Read`] on a failed read.
    pub fn sample(&self, t: i64, lat: f64, lon: f64) -> Result<Option<f64>> {
        let (Some(stencil), Some((a, b, w))) = (self.grid.stencil(lat, lon), self.time.bracket(t))
        else {
            return Ok(None);
        };
        let series = self.stencil_series(a..b + 1, &stencil)?;
        let first = series.first().and_then(|c| stencil.interpolate(*c));
        if a == b {
            return Ok(first);
        }
        let second = series.last().and_then(|c| stencil.interpolate(*c));
        Ok(lerp_time(first, second, w))
    }
}

/// [`OpenVariable::read_cells`] for several variables at once: every chunk
/// of every variable on one pool of at most `concurrency` threads, so the
/// wind's u and v and the waves of one batch download side by side rather
/// than one variable after another. One map of values per request, in
/// order.
///
/// # Errors
/// [`EnvError::Cancelled`] once `cancel` is set; the first failed read.
pub fn read_cells_of(
    requests: &[(&OpenVariable, &[Cell])],
    cancel: &AtomicBool,
    concurrency: usize,
) -> Result<Vec<BTreeMap<Cell, f32>>> {
    let mut out = Vec::with_capacity(requests.len());
    let mut jobs: Vec<(usize, ChunkRequest)> = Vec::new();
    for (i, (var, cells)) in requests.iter().enumerate() {
        let (outside, groups) = var.group(cells)?;
        out.push(outside);
        jobs.extend(groups.into_iter().map(|g| (i, g)));
    }
    let read = crate::parallel::map_bounded(&jobs, concurrency.max(1), |(i, group)| {
        Ok((*i, requests[*i].0.read_group(group, cancel)?))
    })?;
    for (i, values) in read {
        out[i].extend(values);
    }
    Ok(out)
}

/// Reads, side by side on one pool, the head of every blosc chunk the
/// requests' cells lie in that the session does not hold yet; returns how
/// many were read.
///
/// # Errors
/// [`EnvError::Cancelled`] once `cancel` is set; the first failed read.
pub fn prefetch_heads(
    requests: &[(&OpenVariable, &[Cell])],
    cancel: &AtomicBool,
    concurrency: usize,
) -> Result<usize> {
    let mut jobs: Vec<(&OpenVariable, String)> = Vec::new();
    for (var, cells) in requests {
        if var.layout != Layout::Blocks {
            continue;
        }
        let (_, groups) = var.group(cells)?;
        for (_, key, _) in groups {
            if !var.memory.contains(var.namespace(), &key, Part::Head) {
                jobs.push((var, key));
            }
        }
    }
    crate::parallel::map_bounded(&jobs, concurrency.max(1), |(var, key)| {
        if cancel.load(Ordering::SeqCst) {
            return Err(EnvError::Cancelled);
        }
        var.head(key).map(|_| ())
    })?;
    Ok(jobs.len())
}

/// The four cells of a stencil at one step, in [`Stencil::interpolate`]'s
/// corner order.
pub fn corner_cells(step: u64, stencil: &Stencil) -> [Cell; 4] {
    [
        (step, stencil.lat[0], stencil.lon[0]),
        (step, stencil.lat[0], stencil.lon[1]),
        (step, stencil.lat[1], stencil.lon[0]),
        (step, stencil.lat[1], stencil.lon[1]),
    ]
}

/// The four corner values of a stencil at one step, NaN where unread.
pub fn corners_of(values: &BTreeMap<Cell, f32>, step: u64, stencil: &Stencil) -> [f32; 4] {
    corner_cells(step, stencil).map(|cell| values.get(&cell).copied().unwrap_or(f32::NAN))
}

/// Linear interpolation in time between the values at the steps either side
/// (weight `w` on the later). One side missing (an unwritten hour): the
/// nearer side only, and nothing if the missing side is the nearer.
pub fn lerp_time(first: Option<f64>, second: Option<f64>, w: f64) -> Option<f64> {
    match (first, second) {
        (Some(x), Some(y)) => Some(x + (y - x) * w),
        (Some(x), None) if w <= 0.5 => Some(x),
        (None, Some(y)) if w >= 0.5 => Some(y),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dataset_ids_are_distinct_and_urls_are_https() {
        let ids: std::collections::BTreeSet<_> = Dataset::ALL.iter().map(|d| d.id()).collect();
        assert_eq!(ids.len(), Dataset::ALL.len());
        for d in Dataset::ALL {
            assert!(d.url().starts_with("https://"), "{d:?}");
            assert!(!d.version().is_empty());
        }
    }

    /// Wind is "from", currents "toward"; waves and currents are missing
    /// over land, wind is not (CLAUDE.md conventions).
    #[test]
    fn the_variable_table_follows_the_conventions() {
        use vars::*;
        for v in [WB2_U10, WB2_V10, ARCO_U10, ARCO_V10] {
            assert_eq!(v.sense, Sense::From);
            assert!(!v.land_is_missing);
            assert_eq!(v.units, "m s-1");
        }
        assert_eq!(ARCO_MWD.sense, Sense::From);
        for v in [
            CMEMS_UO,
            CMEMS_VO,
            CMEMS_UTIDE,
            CMEMS_VTIDE,
            CMEMS_UTOTAL,
            CMEMS_VTOTAL,
        ] {
            assert_eq!(v.sense, Sense::Toward);
            assert!(v.land_is_missing);
            assert_eq!(v.dataset, Dataset::CmemsGlobalMerged);
        }
    }
}
