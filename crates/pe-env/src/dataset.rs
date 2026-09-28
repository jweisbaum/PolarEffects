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
//! - **Copernicus Marine geoChunked** keeps 4272 hours (about six months) of
//!   a 16 × 8 cell box (1.3° × 0.7°) per chunk: a track costs about one
//!   chunk per box it crosses, whatever its length.

use std::ops::Range;

use zarrs::array::codec::api::CodecOptions;
use zarrs::config::MetadataRetrieveVersion;
use zarrs::group::Group;
use zarrs::storage::ReadableStorage;

use crate::error::{EnvError, Result};
use crate::grid::{Axis, Grid, Stencil};
use crate::store::{
    ReadArray, TimeAxis, dtype_name, open_array, read_axis, read_err, read_time_axis,
};

/// The WeatherBench2 hourly ERA5 store (D12). Frozen at 2023-01-10.
pub const WB2_HOURLY_URL: &str = "https://storage.googleapis.com/weatherbench2/datasets/era5/1959-2023_01_10-full_37-1h-0p25deg-chunk-1.zarr";

/// ARCO-ERA5, updated as ECMWF publishes. Zarr v2 despite the `-v3` in the
/// name.
pub const ARCO_ERA5_URL: &str = "https://storage.googleapis.com/gcp-public-data-arco-era5/ar/full_37-1h-0p25deg-chunk-1.zarr-v3";

/// The Copernicus Marine global merged surface current, hourly, 1/12°,
/// chunked for time series at a place (spec.md 7.5.1 tier 2).
pub const CMEMS_MERGED_GEO_URL: &str = "https://s3.waw3-1.cloudferro.com/mdl-arco-geo-015/arco/GLOBAL_ANALYSISFORECAST_PHY_001_024/cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211/geoChunked.zarr";

/// A dataset: one Zarr store.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Dataset {
    /// WeatherBench2 ERA5, hourly, 0.25°, 1959 to 2023-01-10.
    Wb2Era5Hourly,
    /// ARCO-ERA5, hourly, 0.25°, 1940 to a few days ago.
    ArcoEra5,
    /// Copernicus Marine global merged current, 2020-11 onward.
    CmemsGlobalMerged,
}

impl Dataset {
    /// Every dataset.
    pub const ALL: [Self; 3] = [Self::Wb2Era5Hourly, Self::ArcoEra5, Self::CmemsGlobalMerged];

    /// The name recorded on each sample (`DatasetRecord::name`), and the
    /// chunk cache namespace.
    pub fn id(self) -> &'static str {
        match self {
            Self::Wb2Era5Hourly => "wb2-era5-1h",
            Self::ArcoEra5 => "arco-era5",
            Self::CmemsGlobalMerged => "cmems-glo-merged-uv-geo",
        }
    }

    /// Where it is published.
    pub fn url(self) -> &'static str {
        match self {
            Self::Wb2Era5Hourly => WB2_HOURLY_URL,
            Self::ArcoEra5 => ARCO_ERA5_URL,
            Self::CmemsGlobalMerged => CMEMS_MERGED_GEO_URL,
        }
    }

    /// The version recorded on each sample: the store's own dated name.
    pub fn version(self) -> &'static str {
        match self {
            Self::Wb2Era5Hourly => "1959-2023_01_10-full_37-1h-0p25deg-chunk-1",
            Self::ArcoEra5 => "full_37-1h-0p25deg-chunk-1.zarr-v3",
            Self::CmemsGlobalMerged => "cmems_mod_glo_phy_anfc_merged-uv_PT1H-i_202211",
        }
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

/// The variable table (spec.md 7.5; CLAUDE.md "Adding a reanalysis
/// variable").
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
    /// Eulerian (circulation) current, eastward.
    pub const CMEMS_UO: Variable = v(
        Dataset::CmemsGlobalMerged,
        "uo",
        "m s-1",
        Sense::Toward,
        true,
    );
    /// Eulerian (circulation) current, northward.
    pub const CMEMS_VO: Variable = v(
        Dataset::CmemsGlobalMerged,
        "vo",
        "m s-1",
        Sense::Toward,
        true,
    );
    /// Tidal current, eastward.
    pub const CMEMS_UTIDE: Variable = v(
        Dataset::CmemsGlobalMerged,
        "utide",
        "m s-1",
        Sense::Toward,
        true,
    );
    /// Tidal current, northward.
    pub const CMEMS_VTIDE: Variable = v(
        Dataset::CmemsGlobalMerged,
        "vtide",
        "m s-1",
        Sense::Toward,
        true,
    );
    /// Total current including Stokes drift, eastward.
    pub const CMEMS_UTOTAL: Variable = v(
        Dataset::CmemsGlobalMerged,
        "utotal",
        "m s-1",
        Sense::Toward,
        true,
    );
    /// Total current including Stokes drift, northward.
    pub const CMEMS_VTOTAL: Variable = v(
        Dataset::CmemsGlobalMerged,
        "vtotal",
        "m s-1",
        Sense::Toward,
        true,
    );
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

/// An opened variable: its array, grid and time axis.
pub struct OpenVariable {
    spec: Variable,
    array: ReadArray,
    grid: Grid,
    time: TimeAxis,
    /// Number of dimensions between time and latitude (a level), each read
    /// at index 0.
    middle: usize,
    concurrency: usize,
}

impl std::fmt::Debug for OpenVariable {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenVariable")
            .field("spec", &self.spec)
            .field("grid", &self.grid)
            .field("time", &self.time)
            .finish_non_exhaustive()
    }
}

impl OpenVariable {
    /// Opens `spec`'s array on `store` and reads its axes.
    ///
    /// # Errors
    /// [`EnvError::Layout`] if the array is not `(time, …, latitude,
    /// longitude)` float32 on regular axes.
    pub fn open(store: &ReadableStorage, spec: Variable) -> Result<Self> {
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
        if !dtype_name(&array).contains("float32") {
            return Err(EnvError::Layout(format!(
                "{} is stored as {}; this reader expects float32",
                spec.array,
                dtype_name(&array)
            )));
        }
        let lat = read_axis(store, "/latitude", "the latitude axis")?;
        let lon = read_axis(store, "/longitude", "the longitude axis")?;
        let grid = Grid {
            lat: Axis::from_values("latitude", &lat)?,
            lon: Axis::from_values("longitude", &lon)?,
        };
        let shape = array.shape();
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
        Ok(Self {
            spec,
            array,
            grid,
            time,
            middle: n - 3,
            concurrency: 8,
        })
    }

    /// Limits how many chunks are fetched and decoded at once (spec.md 3.4).
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n.max(1);
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

    /// The stencil values for time steps `steps` at `stencil`, one
    /// `[f32; 4]` per step, NaN where missing.
    ///
    /// # Errors
    /// [`EnvError::Read`] on a failed read.
    pub fn stencil_series(&self, steps: Range<u64>, stencil: &Stencil) -> Result<Vec<[f32; 4]>> {
        let nt = (steps.end - steps.start) as usize;
        let mut out = vec![[f32::NAN; 4]; nt];
        let (i0, i1) = (
            stencil.lat[0].min(stencil.lat[1]),
            stencil.lat[0].max(stencil.lat[1]),
        );
        let lat_rows = (i1 - i0 + 1) as u64;
        // Longitude columns: one contiguous read unless the stencil wraps.
        let columns: Vec<(usize, Vec<usize>)> =
            if stencil.lon[1] == stencil.lon[0] + 1 || stencil.lon[1] == stencil.lon[0] {
                vec![(stencil.lon[0], vec![stencil.lon[0], stencil.lon[1]])]
            } else {
                vec![
                    (stencil.lon[0], vec![stencil.lon[0]]),
                    (stencil.lon[1], vec![stencil.lon[1]]),
                ]
            };
        let options = CodecOptions::default().with_concurrent_target(self.concurrency);
        for (j_start, js) in columns {
            let j_end = js.iter().copied().max().unwrap_or(j_start) as u64 + 1;
            let width = j_end - j_start as u64;
            let mut subset: Vec<Range<u64>> = vec![steps.clone()];
            subset.extend(std::iter::repeat_n(0..1, self.middle));
            subset.push(i0 as u64..i1 as u64 + 1);
            subset.push(j_start as u64..j_end);
            let values = self
                .array
                .retrieve_array_subset_opt::<Vec<f32>>(&subset, &options)
                .map_err(read_err(self.spec.array))?;
            let per_step = (lat_rows * width) as usize;
            for (t, block) in values.chunks(per_step).enumerate().take(nt) {
                for (corner, (li, lj)) in [(0, 0), (0, 1), (1, 0), (1, 1)].into_iter().enumerate() {
                    let row = stencil.lat[li] - i0;
                    let col = stencil.lon[lj];
                    if col < j_start || col as u64 >= j_end {
                        continue;
                    }
                    let v = block[row * width as usize + (col - j_start)];
                    out[t][corner] = if v.abs() >= FILL_MAGNITUDE {
                        f32::NAN
                    } else {
                        v
                    };
                }
            }
        }
        Ok(out)
    }

    /// [`Self::stencil_series`] for scattered steps, one read per step on at
    /// most `concurrency` threads: the shape of a track, whose hours are
    /// known up front and each need their own global chunk.
    ///
    /// # Errors
    /// The first failed read.
    pub fn stencil_at_steps(
        &self,
        steps: &[u64],
        stencil: &Stencil,
        concurrency: usize,
    ) -> Result<Vec<[f32; 4]>> {
        let per_step = crate::parallel::map_bounded(steps, concurrency, |&step| {
            self.stencil_series(step..step + 1, stencil)
        })?;
        Ok(per_step.into_iter().flatten().collect())
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
        Ok(match (first, second) {
            (Some(x), Some(y)) => Some(x + (y - x) * w),
            // One side missing (an unwritten hour): the nearer side only.
            (Some(x), None) if w <= 0.5 => Some(x),
            (None, Some(y)) if w >= 0.5 => Some(y),
            _ => None,
        })
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
