//! A read-only HTTP store for `zarrs`. Ported from VectorEffects' `ve-zarr`.
//!
//! `zarrs_http` does this already, but it takes `reqwest` with its default
//! features — `native-tls`, which on Linux is `openssl-sys`, a system C
//! library the five-target build forbids (D6). The trait is three methods over
//! `GET`, so it is written here against the client in [`crate::net`].
//!
//! Reads only: nothing here can write, and the stores are anonymous and
//! public, so no credential ever reaches them. Every body read is counted in
//! [`NetStats`], which is what the pre-flight estimate and the M3 numbers
//! are measured with.

use std::str::FromStr;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use reqwest::header::{CONTENT_LENGTH, HeaderValue, RANGE};
use reqwest::{StatusCode, Url};
use zarrs_storage::Bytes;
use zarrs_storage::byte_range::ByteRangeIterator;
use zarrs_storage::{
    MaybeBytes, MaybeBytesIterator, ReadableStorageTraits, StorageError, StoreKey,
};

use crate::error::{EnvError, Result};

/// Counters for what went over the network.
#[derive(Debug, Default)]
pub struct NetStats {
    /// Requests sent, including ones answered "missing".
    pub requests: AtomicU64,
    /// Body bytes received.
    pub bytes: AtomicU64,
    /// Keys the archive answered 403 or 404 for.
    pub missing: AtomicU64,
}

impl NetStats {
    /// `(requests, bytes, missing)` at this moment.
    pub fn snapshot(&self) -> (u64, u64, u64) {
        (
            self.requests.load(Ordering::Relaxed),
            self.bytes.load(Ordering::Relaxed),
            self.missing.load(Ordering::Relaxed),
        )
    }
}

/// A Zarr store read over HTTPS.
#[derive(Debug)]
pub struct HttpStore {
    base: Url,
    client: reqwest::blocking::Client,
    stats: Arc<NetStats>,
}

fn failed(err: reqwest::Error) -> StorageError {
    StorageError::Other(err.to_string())
}

impl HttpStore {
    /// Opens a store at `base`, which must be an absolute URL, with the
    /// given per-request timeout (spec.md 3.4).
    ///
    /// # Errors
    /// [`EnvError::Open`] if `base` is not a URL or no client can be built.
    pub fn new(base: &str, timeout: Duration) -> Result<Self> {
        let base = Url::from_str(base)
            .map_err(|err| EnvError::Open(format!("{base} is not a URL: {err}")))?;
        Ok(Self {
            base,
            client: crate::net::client(timeout)?,
            stats: Arc::new(NetStats::default()),
        })
    }

    /// The counters for this store.
    pub fn stats(&self) -> Arc<NetStats> {
        Arc::clone(&self.stats)
    }

    /// The URL a key is read from.
    fn url(&self, key: &StoreKey) -> std::result::Result<Url, StorageError> {
        let mut url = self.base.as_str().trim_end_matches('/').to_owned();
        let key = key.as_str();
        if !key.is_empty() {
            url.push('/');
            url.push_str(key.strip_prefix('/').unwrap_or(key));
        }
        Url::parse(&url).map_err(|err| StorageError::Other(err.to_string()))
    }

    fn body(
        &self,
        response: reqwest::blocking::Response,
    ) -> std::result::Result<Bytes, StorageError> {
        let bytes = response.bytes().map_err(failed)?;
        self.stats
            .bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(bytes)
    }

    fn sent(&self) {
        self.stats.requests.fetch_add(1, Ordering::Relaxed);
    }

    fn missing(&self) {
        self.stats.missing.fetch_add(1, Ordering::Relaxed);
    }
}

impl ReadableStorageTraits for HttpStore {
    fn get(&self, key: &StoreKey) -> std::result::Result<MaybeBytes, StorageError> {
        self.sent();
        let response = self.client.get(self.url(key)?).send().map_err(failed)?;
        match response.status() {
            StatusCode::OK => Ok(Some(self.body(response)?)),
            // A key that is not there. The S3 endpoint behind Copernicus
            // Marine answers a missing key with 403 rather than 404, and a
            // Zarr reader must read both as "no such chunk" or every probe
            // for an optional key becomes an error (spec.md 7.5.1).
            StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => {
                self.missing();
                Ok(None)
            }
            status => Err(StorageError::Other(format!(
                "the archive answered {status} for {}",
                key.as_str()
            ))),
        }
    }

    /// One range request per range, rather than one multipart request for all
    /// of them. A server may answer a multipart request with the whole
    /// object; the ranges asked for here are few, so the simple form costs
    /// little and cannot be misread.
    fn get_partial_many<'a>(
        &'a self,
        key: &StoreKey,
        byte_ranges: ByteRangeIterator<'a>,
    ) -> std::result::Result<MaybeBytesIterator<'a>, StorageError> {
        let Some(size) = self.size_key(key)? else {
            return Ok(None);
        };
        let url = self.url(key)?;
        let mut out: Vec<std::result::Result<Bytes, StorageError>> = Vec::new();
        for range in byte_ranges {
            let (start, end) = (range.start(size), range.end(size));
            let header = HeaderValue::from_str(&format!("bytes={start}-{}", end.saturating_sub(1)))
                .map_err(|err| StorageError::Other(err.to_string()))?;
            self.sent();
            let response = self
                .client
                .get(url.clone())
                .header(RANGE, header)
                .send()
                .map_err(failed)?;
            match response.status() {
                StatusCode::PARTIAL_CONTENT => out.push(Ok(self.body(response)?)),
                // A server that ignored the range sent the whole object.
                StatusCode::OK => {
                    let whole = self.body(response)?;
                    let (start, end) = (start as usize, end as usize);
                    if end > whole.len() {
                        return Err(StorageError::Other(
                            "the archive returned less than the range asked for".to_owned(),
                        ));
                    }
                    out.push(Ok(whole.slice(start..end)));
                }
                status => {
                    return Err(StorageError::Other(format!(
                        "the archive answered {status} for a range of {}",
                        key.as_str()
                    )));
                }
            }
        }
        Ok(Some(Box::new(out.into_iter())))
    }

    fn size_key(&self, key: &StoreKey) -> std::result::Result<Option<u64>, StorageError> {
        self.sent();
        let response = self.client.head(self.url(key)?).send().map_err(failed)?;
        match response.status() {
            StatusCode::OK => response
                .headers()
                .get(CONTENT_LENGTH)
                .and_then(|value| value.to_str().ok())
                .and_then(|value| u64::from_str(value).ok())
                .map(Some)
                .ok_or_else(|| StorageError::Other("no content length".to_owned())),
            StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => {
                self.missing();
                Ok(None)
            }
            status => Err(StorageError::Other(format!(
                "the archive answered {status} for the size of {}",
                key.as_str()
            ))),
        }
    }

    fn supports_get_partial(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The store is read-only and anonymous, and its keys hang off the base
    /// URL exactly as the archive lays them out. A key joined wrongly reads
    /// somebody else's object or nothing at all, and the failure would look
    /// like an empty archive rather than a bug.
    #[test]
    fn a_key_hangs_off_the_base_url() {
        let store = HttpStore::new(
            "https://example.invalid/store.zarr/",
            Duration::from_secs(5),
        )
        .expect("a URL");
        let url = |key: &str| {
            store
                .url(&StoreKey::new(key).expect("a key"))
                .expect("a URL")
                .to_string()
        };
        assert_eq!(
            url("10m_u_component_of_wind/.zarray"),
            "https://example.invalid/store.zarr/10m_u_component_of_wind/.zarray"
        );
        assert_eq!(
            url("utotal/9.0.97.262"),
            "https://example.invalid/store.zarr/utotal/9.0.97.262"
        );
    }

    #[test]
    fn a_base_that_is_not_a_url_is_refused() {
        assert!(HttpStore::new("not a url", Duration::from_secs(5)).is_err());
    }
}
