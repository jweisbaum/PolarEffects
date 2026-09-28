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

use std::io::Read;
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

/// The largest body any read may return. The biggest object any store here
/// serves is a global ERA5 chunk of about 3.5 MB; a body many times that is
/// a misbehaving server, and reading it whole would only exhaust memory.
pub const MAX_BODY_BYTES: u64 = 64 << 20;

/// How many times a transient failure is tried again before it is reported.
pub const RETRIES: u32 = 3;

/// The wait before the first retry; each later one waits twice as long.
pub const FIRST_BACKOFF: Duration = Duration::from_millis(500);

/// Counters for what went over the network.
#[derive(Debug, Default)]
pub struct NetStats {
    /// Requests sent, including ones answered "missing".
    pub requests: AtomicU64,
    /// Body bytes received.
    pub bytes: AtomicU64,
    /// Keys the archive answered 403 or 404 for.
    pub missing: AtomicU64,
    /// Requests sent again after a transient failure.
    pub retries: AtomicU64,
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

/// What an archive's answer means for the read (carried from M3).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The body is the object.
    Ok,
    /// No such object: "no data", never an error (spec.md 7.5.1).
    Missing,
    /// Worth trying again after a pause: the server is busy or failing.
    Transient,
    /// Trying again would give the same answer.
    Permanent,
}

/// Classifies a status. A missing key on the S3 endpoint behind Copernicus
/// Marine is 403 rather than 404, and both are "no data". 5xx, 429 (too
/// many requests) and 408 (request timeout) are transient; any other
/// status is permanent.
pub fn classify(status: StatusCode) -> Answer {
    match status {
        StatusCode::OK | StatusCode::PARTIAL_CONTENT => Answer::Ok,
        StatusCode::NOT_FOUND | StatusCode::FORBIDDEN => Answer::Missing,
        StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => Answer::Transient,
        s if s.is_server_error() => Answer::Transient,
        _ => Answer::Permanent,
    }
}

/// Whether a failure to get any answer at all is worth retrying: timeouts,
/// refused or dropped connections and bodies cut short are; a malformed
/// request or a redirect loop is not.
fn transient_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request() || err.is_body() || err.is_decode()
}

/// A failed attempt, and whether another is worth making.
struct Failure {
    transient: bool,
    message: String,
}

impl Failure {
    fn of(err: &reqwest::Error) -> Self {
        Self {
            transient: transient_error(err),
            message: err.to_string(),
        }
    }
}

/// A Zarr store read over HTTPS.
#[derive(Debug)]
pub struct HttpStore {
    base: Url,
    client: reqwest::blocking::Client,
    stats: Arc<NetStats>,
    backoff: Duration,
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
            backoff: FIRST_BACKOFF,
        })
    }

    /// The same store with a different first retry pause (tests use a short
    /// one).
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
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

    /// Reads a body, refusing one larger than [`MAX_BODY_BYTES`] before
    /// and while reading it.
    fn body(&self, response: reqwest::blocking::Response) -> std::result::Result<Bytes, Failure> {
        if response
            .content_length()
            .is_some_and(|n| n > MAX_BODY_BYTES)
        {
            return Err(Failure {
                transient: false,
                message: format!(
                    "the archive offered {} bytes, more than the {MAX_BODY_BYTES}-byte limit",
                    response.content_length().unwrap_or_default()
                ),
            });
        }
        let mut bytes = Vec::new();
        response
            .take(MAX_BODY_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|e| Failure {
                transient: true,
                message: format!("the body was cut short: {e}"),
            })?;
        if bytes.len() as u64 > MAX_BODY_BYTES {
            return Err(Failure {
                transient: false,
                message: format!("the body is larger than the {MAX_BODY_BYTES}-byte limit"),
            });
        }
        self.stats
            .bytes
            .fetch_add(bytes.len() as u64, Ordering::Relaxed);
        Ok(Bytes::from(bytes))
    }

    /// Runs `attempt` until it succeeds, fails permanently, or has been
    /// retried [`RETRIES`] times, pausing longer before each retry.
    fn with_retries<T>(
        &self,
        what: &str,
        mut attempt: impl FnMut() -> std::result::Result<T, Failure>,
    ) -> std::result::Result<T, StorageError> {
        let mut pause = self.backoff;
        let mut tries = 0;
        loop {
            self.stats.requests.fetch_add(1, Ordering::Relaxed);
            match attempt() {
                Ok(value) => return Ok(value),
                Err(failure) if failure.transient && tries < RETRIES => {
                    tries += 1;
                    self.stats.retries.fetch_add(1, Ordering::Relaxed);
                    std::thread::sleep(pause);
                    pause = pause.saturating_mul(2);
                }
                Err(failure) => {
                    let kind = if failure.transient {
                        "transient, retries exhausted"
                    } else {
                        "permanent"
                    };
                    return Err(StorageError::Other(format!(
                        "{what}: {} ({kind})",
                        failure.message
                    )));
                }
            }
        }
    }

    fn missing(&self) {
        self.stats.missing.fetch_add(1, Ordering::Relaxed);
    }

    fn status_failure(status: StatusCode, what: &str) -> Failure {
        Failure {
            transient: classify(status) == Answer::Transient,
            message: format!("the archive answered {status} for {what}"),
        }
    }
}

impl ReadableStorageTraits for HttpStore {
    fn get(&self, key: &StoreKey) -> std::result::Result<MaybeBytes, StorageError> {
        let url = self.url(key)?;
        self.with_retries(key.as_str(), || {
            let response = self
                .client
                .get(url.clone())
                .send()
                .map_err(|e| Failure::of(&e))?;
            let status = response.status();
            match classify(status) {
                Answer::Ok => Ok(Some(self.body(response)?)),
                Answer::Missing => {
                    self.missing();
                    Ok(None)
                }
                _ => Err(Self::status_failure(status, key.as_str())),
            }
        })
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
            let part = self.with_retries(key.as_str(), || {
                let response = self
                    .client
                    .get(url.clone())
                    .header(RANGE, header.clone())
                    .send()
                    .map_err(|e| Failure::of(&e))?;
                match response.status() {
                    StatusCode::PARTIAL_CONTENT => self.body(response),
                    // A server that ignored the range sent the whole object.
                    StatusCode::OK => {
                        let whole = self.body(response)?;
                        let (start, end) = (start as usize, end as usize);
                        if end > whole.len() {
                            return Err(Failure {
                                transient: false,
                                message: "the archive returned less than the range asked for"
                                    .to_owned(),
                            });
                        }
                        Ok(whole.slice(start..end))
                    }
                    status => Err(Self::status_failure(status, key.as_str())),
                }
            })?;
            out.push(Ok(part));
        }
        Ok(Some(Box::new(out.into_iter())))
    }

    fn size_key(&self, key: &StoreKey) -> std::result::Result<Option<u64>, StorageError> {
        let url = self.url(key)?;
        self.with_retries(key.as_str(), || {
            let response = self
                .client
                .head(url.clone())
                .send()
                .map_err(|e| Failure::of(&e))?;
            let status = response.status();
            match classify(status) {
                Answer::Ok => response
                    .headers()
                    .get(CONTENT_LENGTH)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| u64::from_str(value).ok())
                    .map(Some)
                    .ok_or_else(|| Failure {
                        transient: false,
                        message: "no content length".to_owned(),
                    }),
                Answer::Missing => {
                    self.missing();
                    Ok(None)
                }
                _ => Err(Self::status_failure(status, key.as_str())),
            }
        })
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

    /// A local server answering each connection with the next canned
    /// response, and counting the connections.
    fn serve(responses: Vec<String>) -> (String, std::thread::JoinHandle<usize>) {
        use std::io::Write;
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let addr = listener.local_addr().expect("an address");
        let handle = std::thread::spawn(move || {
            let mut served = 0;
            for response in responses {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let _ = stream.read(&mut buf);
                let _ = stream.write_all(response.as_bytes());
                served += 1;
            }
            served
        });
        (format!("http://127.0.0.1:{}/store", addr.port()), handle)
    }

    fn reply(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn store(url: &str) -> HttpStore {
        HttpStore::new(url, Duration::from_secs(5))
            .expect("a store")
            .with_backoff(Duration::from_millis(1))
    }

    #[test]
    fn statuses_are_classified() {
        assert_eq!(classify(StatusCode::OK), Answer::Ok);
        assert_eq!(classify(StatusCode::NOT_FOUND), Answer::Missing);
        assert_eq!(classify(StatusCode::FORBIDDEN), Answer::Missing);
        assert_eq!(classify(StatusCode::SERVICE_UNAVAILABLE), Answer::Transient);
        assert_eq!(classify(StatusCode::BAD_GATEWAY), Answer::Transient);
        assert_eq!(classify(StatusCode::TOO_MANY_REQUESTS), Answer::Transient);
        assert_eq!(classify(StatusCode::BAD_REQUEST), Answer::Permanent);
        assert_eq!(classify(StatusCode::UNAUTHORIZED), Answer::Permanent);
    }

    /// A busy server is asked again, and the read succeeds once it answers.
    #[test]
    fn a_transient_failure_is_retried() {
        let (url, server) = serve(vec![
            reply("503 Service Unavailable", ""),
            reply("429 Too Many Requests", ""),
            reply("200 OK", "chunk"),
        ]);
        let store = store(&url);
        let got = store
            .get(&StoreKey::new("u/0.0.0").expect("a key"))
            .expect("read")
            .expect("present");
        assert_eq!(&got[..], b"chunk");
        assert_eq!(store.stats.retries.load(Ordering::Relaxed), 2);
        assert_eq!(server.join().expect("server"), 3);
    }

    /// Retries are bounded: a server that keeps failing is reported.
    #[test]
    fn retries_are_bounded() {
        let (url, server) = serve(vec![reply("500 Internal Server Error", ""); 4]);
        let store = store(&url);
        let err = store
            .get(&StoreKey::new("u/0.0.0").expect("a key"))
            .expect_err("gives up");
        assert!(err.to_string().contains("retries exhausted"), "{err}");
        assert_eq!(server.join().expect("server"), 4);
    }

    /// A permanent failure is not retried, and "missing" is not a failure.
    #[test]
    fn a_permanent_failure_is_not_retried_and_missing_is_no_data() {
        let (url, server) = serve(vec![
            reply("400 Bad Request", ""),
            reply("403 Forbidden", ""),
        ]);
        let store = store(&url);
        let key = StoreKey::new("u/0.0.0").expect("a key");
        let err = store.get(&key).expect_err("refused");
        assert!(err.to_string().contains("permanent"), "{err}");
        assert_eq!(store.get(&key).expect("read"), None);
        assert_eq!(store.stats.snapshot().2, 1);
        assert_eq!(server.join().expect("server"), 2);
    }

    /// A body announced larger than the cap is refused without reading it.
    #[test]
    fn an_oversized_body_is_refused() {
        let huge = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\nxx",
            MAX_BODY_BYTES + 1
        );
        let (url, server) = serve(vec![huge]);
        let store = store(&url);
        let err = store
            .get(&StoreKey::new("u/0.0.0").expect("a key"))
            .expect_err("refused");
        assert!(err.to_string().contains("limit"), "{err}");
        assert_eq!(server.join().expect("server"), 1);
    }

    #[test]
    fn a_base_that_is_not_a_url_is_refused() {
        assert!(HttpStore::new("not a url", Duration::from_secs(5)).is_err());
    }
}
