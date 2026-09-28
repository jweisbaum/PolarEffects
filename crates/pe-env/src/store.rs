//! What every dataset reader shares: opening a store (over HTTPS through the
//! chunk cache, or from a folder), opening an array, and reading coordinate
//! and time axes. Adapted from VectorEffects' `ve-zarr/src/store.rs`.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use zarrs::array::Array;
use zarrs::config::MetadataRetrieveVersion;
use zarrs::storage::{ReadableStorage, ReadableStorageTraits};

use crate::cache::{CachedStore, ChunkCache};
use crate::error::{EnvError, Result};
use crate::http::{HttpStore, NetStats};
use crate::time::TimeUnits;

/// A Zarr array on a read-only store.
pub type ReadArray = Array<dyn ReadableStorageTraits>;

/// An opened store and the counters for its network use.
#[derive(Clone)]
pub struct OpenStore {
    /// The store, ready for `zarrs`.
    pub store: ReadableStorage,
    /// Network counters; `None` for a store on disk.
    pub net: Option<Arc<NetStats>>,
    /// The chunk cache it reads through, and its namespace there.
    pub cache: Option<(Arc<ChunkCache>, String)>,
}

impl std::fmt::Debug for OpenStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("OpenStore")
            .field("net", &self.net.as_ref().map(|n| n.snapshot()))
            .finish_non_exhaustive()
    }
}

/// Opens an anonymous HTTP store, answering chunk reads from `cache` first
/// when there is one.
///
/// # Errors
/// [`EnvError::Open`] if the URL is bad or no client can be built.
pub fn open_http(
    url: &str,
    timeout: Duration,
    cache: Option<(Arc<ChunkCache>, &str)>,
) -> Result<OpenStore> {
    open_http_interruptible(
        url,
        timeout,
        cache,
        Arc::new(crate::http::Interrupt::default()),
    )
}

/// [`open_http`], its retry pauses ending as soon as `interrupt` is set.
///
/// # Errors
/// As [`open_http`].
pub fn open_http_interruptible(
    url: &str,
    timeout: Duration,
    cache: Option<(Arc<ChunkCache>, &str)>,
    interrupt: Arc<crate::http::Interrupt>,
) -> Result<OpenStore> {
    crate::codec::register();
    let http = HttpStore::new(url, timeout)?.with_interrupt(interrupt);
    let net = Some(http.stats());
    let kept = cache
        .as_ref()
        .map(|(cache, namespace)| (Arc::clone(cache), (*namespace).to_owned()));
    let store: ReadableStorage = match cache {
        Some((cache, namespace)) => Arc::new(CachedStore::new(http, cache, namespace)),
        None => Arc::new(http),
    };
    Ok(OpenStore {
        store,
        net,
        cache: kept,
    })
}

/// Opens a store kept in a folder: a recorded fixture, in tests.
///
/// # Errors
/// [`EnvError::Open`] if the folder cannot be opened.
pub fn open_dir(path: &Path) -> Result<OpenStore> {
    crate::codec::register();
    let store = zarrs::filesystem::FilesystemStore::new(path)
        .map_err(|e| EnvError::Open(format!("{}: {e}", path.display())))?;
    Ok(OpenStore {
        store: Arc::new(store),
        net: None,
        cache: None,
    })
}

/// Opens an array, insisting on Zarr V2.
///
/// The default open probes for a V3 `zarr.json` first. Google Cloud answers a
/// missing key with 404 and the probe moves on; the CloudFerro S3 behind
/// Copernicus Marine answers 403. Every store read here is V2, so the probe
/// is skipped rather than interpreted.
///
/// # Errors
/// [`EnvError::Open`] if the metadata cannot be read.
pub fn open_array(store: &ReadableStorage, path: &str) -> Result<ReadArray> {
    Array::open_opt(store.clone(), path, &MetadataRetrieveVersion::V2)
        .map_err(|e| EnvError::Open(format!("{path}: {e}")))
}

/// Wraps a zarrs read error with what was being read.
pub fn read_err(what: &str) -> impl FnOnce(zarrs::array::ArrayError) -> EnvError + '_ {
    move |source| EnvError::Read {
        what: what.to_string(),
        source: Box::new(source),
    }
}

/// The array's data type name, lower case, for dispatching on.
pub fn dtype_name(array: &ReadArray) -> String {
    format!("{:?}", array.data_type()).to_ascii_lowercase()
}

/// Reads `range` of a one-dimensional numeric array as `f64`, whatever
/// numeric type it is stored as.
///
/// # Errors
/// [`EnvError::Layout`] for a non-numeric type, [`EnvError::Read`] on a
/// failed read.
pub fn read_1d_f64(array: &ReadArray, range: std::ops::Range<u64>, what: &str) -> Result<Vec<f64>> {
    let subset = vec![range];
    let dtype = dtype_name(array);
    macro_rules! read_as {
        ($t:ty) => {{
            let v = array
                .retrieve_array_subset::<Vec<$t>>(&subset)
                .map_err(read_err(what))?;
            Ok(v.into_iter().map(|x| x as f64).collect())
        }};
    }
    if dtype.contains("float32") {
        read_as!(f32)
    } else if dtype.contains("float64") {
        read_as!(f64)
    } else if dtype.contains("int16") {
        read_as!(i16)
    } else if dtype.contains("int32") {
        read_as!(i32)
    } else if dtype.contains("int64") {
        read_as!(i64)
    } else {
        Err(EnvError::Layout(format!(
            "{what} is stored as {dtype}, which this reader does not handle"
        )))
    }
}

/// Reads a whole one-dimensional coordinate array.
///
/// # Errors
/// As [`read_1d_f64`], or [`EnvError::Layout`] if it is not one-dimensional.
pub fn read_axis(store: &ReadableStorage, path: &str, what: &str) -> Result<Vec<f64>> {
    let array = open_array(store, path)?;
    let [len] = array.shape() else {
        return Err(EnvError::Layout(format!(
            "{path} has shape {:?}, expected one dimension",
            array.shape()
        )));
    };
    read_1d_f64(&array, 0..*len, what)
}

/// A regular time axis, in UTC epoch seconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimeAxis {
    /// Time of index 0.
    pub first: i64,
    /// Seconds between steps.
    pub step: i64,
    /// Number of steps.
    pub len: u64,
}

impl TimeAxis {
    /// The time of step `i`.
    pub fn at(&self, i: u64) -> i64 {
        self.first + self.step * i as i64
    }

    /// The time of the last step.
    pub fn last(&self) -> i64 {
        self.at(self.len.saturating_sub(1))
    }

    /// The steps either side of `t` and the weight of the later one, or
    /// `None` if `t` is outside the axis. On a step exactly, both are it.
    pub fn bracket(&self, t: i64) -> Option<(u64, u64, f64)> {
        if self.len == 0 || t < self.first || t > self.last() {
            return None;
        }
        let offset = t - self.first;
        let i = (offset / self.step) as u64;
        let rem = offset % self.step;
        if rem == 0 {
            return Some((i, i, 0.0));
        }
        Some((i, i + 1, rem as f64 / self.step as f64))
    }
}

/// Reads a time axis and checks it is regular.
///
/// ARCO-ERA5's axis has 1.3 million entries in 20 chunks. Reading all of it
/// on every open costs twenty requests for no information, because the axis
/// is a plain count of hours; so the first chunk is read and checked to be
/// evenly spaced, and the very last value is read and checked to be where
/// that spacing puts it. A gap anywhere would move the last value.
///
/// # Errors
/// [`EnvError::Layout`] for missing units, irregular steps or a last value
/// that disagrees; [`EnvError::Read`] on a failed read.
pub fn read_time_axis(store: &ReadableStorage, path: &str) -> Result<TimeAxis> {
    let array = open_array(store, path)?;
    let [len] = array.shape() else {
        return Err(EnvError::Layout(format!(
            "the time axis has shape {:?}, expected one dimension",
            array.shape()
        )));
    };
    let len = *len;
    let units = array
        .attributes()
        .get("units")
        .and_then(|v| v.as_str())
        .ok_or_else(|| EnvError::Layout("the time axis has no units attribute".into()))?;
    let units = TimeUnits::parse(units)?;
    let chunk = array
        .chunk_shape(&[0])
        .map_err(|e| EnvError::Layout(format!("the time axis chunking: {e}")))?;
    let head_len = chunk.first().map_or(len, |n| n.get()).min(len);
    if head_len < 2 {
        return Err(EnvError::Layout(
            "the time axis has fewer than two steps".into(),
        ));
    }
    let head = read_1d_f64(&array, 0..head_len, "the time axis")?;
    let first = units.to_epoch(head[0], 0)?;
    let step = units.to_epoch(head[1], 1)? - first;
    if step <= 0 {
        return Err(EnvError::Layout("the time axis is not increasing".into()));
    }
    for (i, &v) in head.iter().enumerate() {
        if units.to_epoch(v, i)? != first + step * i as i64 {
            return Err(EnvError::Layout(format!(
                "the time axis is not evenly spaced at entry {i}"
            )));
        }
    }
    let axis = TimeAxis { first, step, len };
    if len > head_len {
        let tail = read_1d_f64(&array, len - 1..len, "the end of the time axis")?;
        let last = units.to_epoch(tail[0], (len - 1) as usize)?;
        if last != axis.last() {
            return Err(EnvError::Layout(format!(
                "the time axis ends at {}, not {} as its spacing implies",
                crate::time::to_iso(last),
                crate::time::to_iso(axis.last())
            )));
        }
    }
    Ok(axis)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_time_between_steps_is_bracketed_with_its_weight() {
        let axis = TimeAxis {
            first: 0,
            step: 3600,
            len: 48,
        };
        assert_eq!(axis.bracket(0), Some((0, 0, 0.0)));
        assert_eq!(axis.bracket(7200), Some((2, 2, 0.0)));
        assert_eq!(axis.bracket(7200 + 900), Some((2, 3, 0.25)));
        assert_eq!(axis.bracket(-1), None);
        assert_eq!(axis.bracket(axis.last()), Some((47, 47, 0.0)));
        assert_eq!(axis.bracket(axis.last() + 1), None);
    }
}
