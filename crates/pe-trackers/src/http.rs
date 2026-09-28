//! Reading one tracker response: a body cap, bounded retries of transient
//! failures, progress and cancel (spec.md 7.2, 7.7).
//!
//! The same rules as `pe-env`'s HTTP store (spec.md 7.7): a 5xx or 429
//! answer, a timeout or a dropped connection is tried again three times,
//! pausing 0.5, 1 and 2 s, and anything else fails at once. The two network
//! crates never depend on each other (CLAUDE.md dependency direction), so
//! the small classification is written again here rather than shared.

use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use reqwest::StatusCode;

use crate::error::{Result, TrackerError};

/// The largest body a tracker response may have. The biggest expected is
/// YellowBrick's KML fallback, which is text and about seventeen times the
/// binary: Fastnet 2025's is 98.8 MB (its `AllPositions3` 5.7 MB). A body
/// beyond this is a misbehaving server, and reading it whole would only
/// exhaust memory.
pub const MAX_BODY_BYTES: u64 = 256 << 20;

/// How many times a transient failure is tried again before it is reported.
pub const RETRIES: u32 = 3;

/// The wait before the first retry; each later one waits twice as long.
pub const FIRST_BACKOFF: Duration = Duration::from_millis(500);

/// How often a pause or a body read looks at the cancel flag.
const PAUSE_SLICE: Duration = Duration::from_millis(50);

/// The piece a body is read in, so a cancel and progress land in time.
const READ_PIECE: usize = 64 << 10;

/// What an answer's status means for the read.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// The body is the response.
    Ok,
    /// Worth asking again after a pause: the server is busy or failing.
    Transient,
    /// Asking again would give the same answer.
    Permanent,
}

/// Classifies a status: 5xx, 429 (too many requests) and 408 (request
/// timeout) are transient; any other failure is permanent.
pub fn classify(status: StatusCode) -> Answer {
    match status {
        s if s.is_success() => Answer::Ok,
        StatusCode::TOO_MANY_REQUESTS | StatusCode::REQUEST_TIMEOUT => Answer::Transient,
        s if s.is_server_error() => Answer::Transient,
        _ => Answer::Permanent,
    }
}

/// Whether a failure to get any answer is worth retrying: timeouts, refused
/// or dropped connections and bodies cut short are; a malformed request or
/// a redirect loop is not.
fn transient_error(err: &reqwest::Error) -> bool {
    err.is_timeout() || err.is_connect() || err.is_request() || err.is_body() || err.is_decode()
}

/// An error with its causes, which is where reqwest keeps the reason (a
/// refused redirect, a TLS failure).
fn chain(err: &dyn std::error::Error) -> String {
    let mut text = err.to_string();
    let mut source = err.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}

/// One failed attempt.
enum Failure {
    Transient(String),
    Permanent(String),
    Cancelled,
}

/// Reads tracker responses for one download, which one cancel flag stops.
#[derive(Debug, Clone)]
pub struct Fetcher {
    client: reqwest::blocking::Client,
    cancel: Arc<AtomicBool>,
    backoff: Duration,
    tracker: &'static str,
}

impl Fetcher {
    /// A fetcher with the per-request `timeout` (spec.md 3.4) that stops
    /// when `cancel` is set.
    ///
    /// # Errors
    /// [`TrackerError::Network`] if no HTTP client can be built.
    pub fn new(tracker: &'static str, timeout: Duration, cancel: Arc<AtomicBool>) -> Result<Self> {
        Ok(Self {
            client: crate::net::client(timeout)?,
            cancel,
            backoff: FIRST_BACKOFF,
            tracker,
        })
    }

    /// The same fetcher with a different first retry pause (tests use a
    /// short one).
    pub fn with_backoff(mut self, backoff: Duration) -> Self {
        self.backoff = backoff;
        self
    }

    /// Whether the download has been cancelled.
    pub fn cancelled(&self) -> bool {
        self.cancel.load(Ordering::SeqCst)
    }

    /// Fails with [`TrackerError::Cancelled`] once cancelled.
    ///
    /// # Errors
    /// [`TrackerError::Cancelled`].
    pub fn check(&self) -> Result<()> {
        if self.cancelled() {
            Err(TrackerError::Cancelled)
        } else {
            Ok(())
        }
    }

    /// GETs `url`, retrying transient failures, calling `progress` with the
    /// bytes read so far and the length the server announced.
    ///
    /// # Errors
    /// [`TrackerError::Cancelled`], [`TrackerError::Unavailable`] once the
    /// retries are spent, [`TrackerError::Network`] for a permanent failure.
    pub fn get(&self, url: &str, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<Vec<u8>> {
        self.read(url, None, progress)
    }

    /// [`Fetcher::get`] with its own per-request timeout, for a response the
    /// server builds slowly.
    ///
    /// # Errors
    /// As [`Fetcher::get`].
    pub fn get_with_timeout(
        &self,
        url: &str,
        timeout: Duration,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>> {
        self.read(url, Some(timeout), progress)
    }

    fn read(
        &self,
        url: &str,
        timeout: Option<Duration>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> Result<Vec<u8>> {
        let mut pause = self.backoff;
        let mut tries = 0;
        loop {
            self.check()?;
            match self.attempt(url, timeout, progress) {
                Ok(bytes) => return Ok(bytes),
                Err(Failure::Cancelled) => return Err(TrackerError::Cancelled),
                Err(Failure::Transient(_)) if tries < RETRIES => {
                    tries += 1;
                    // Paused in slices, so a cancel ends the wait at once.
                    let until = Instant::now() + pause;
                    while Instant::now() < until {
                        self.check()?;
                        std::thread::sleep(PAUSE_SLICE.min(until - Instant::now()));
                    }
                    pause = pause.saturating_mul(2);
                }
                Err(Failure::Transient(why)) => {
                    return Err(TrackerError::Unavailable {
                        tracker: self.tracker,
                        why: format!("{why} (tried {} times)", RETRIES + 1),
                    });
                }
                Err(Failure::Permanent(why)) => return Err(TrackerError::Network(why)),
            }
        }
    }

    fn attempt(
        &self,
        url: &str,
        timeout: Option<Duration>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> std::result::Result<Vec<u8>, Failure> {
        let mut request = self.client.get(url);
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().map_err(|e| {
            let why = format!("{} could not be reached: {}", self.tracker, chain(&e));
            if transient_error(&e) {
                Failure::Transient(why)
            } else {
                Failure::Permanent(why)
            }
        })?;
        let status = response.status();
        match classify(status) {
            Answer::Ok => {}
            Answer::Transient => {
                return Err(Failure::Transient(format!(
                    "{} answered {status} for {url}",
                    self.tracker
                )));
            }
            Answer::Permanent => {
                return Err(Failure::Permanent(format!(
                    "{} answered {status} for {url}",
                    self.tracker
                )));
            }
        }
        let total = response.content_length();
        if total.is_some_and(|n| n > MAX_BODY_BYTES) {
            return Err(Failure::Permanent(format!(
                "{} offered {} bytes for {url}, more than the {MAX_BODY_BYTES}-byte limit",
                self.tracker,
                total.unwrap_or_default()
            )));
        }
        let mut body = Vec::with_capacity(
            total
                .and_then(|n| usize::try_from(n).ok())
                .unwrap_or(0)
                .min(READ_PIECE * 16),
        );
        let mut reader = response.take(MAX_BODY_BYTES + 1);
        let mut piece = vec![0u8; READ_PIECE];
        progress(0, total);
        loop {
            if self.cancelled() {
                return Err(Failure::Cancelled);
            }
            let n = reader.read(&mut piece).map_err(|e| {
                Failure::Transient(format!("{}'s answer was cut short: {e}", self.tracker))
            })?;
            if n == 0 {
                break;
            }
            body.extend_from_slice(&piece[..n]);
            if body.len() as u64 > MAX_BODY_BYTES {
                return Err(Failure::Permanent(format!(
                    "{}'s answer for {url} is larger than the {MAX_BODY_BYTES}-byte limit",
                    self.tracker
                )));
            }
            progress(body.len() as u64, total);
        }
        // Complete, whether or not the server announced a length.
        progress(body.len() as u64, Some(body.len() as u64));
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A local server answering each connection with the next canned
    /// response, and counting the connections.
    fn serve(responses: Vec<String>) -> (String, std::thread::JoinHandle<usize>) {
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
        (format!("http://127.0.0.1:{}/x", addr.port()), handle)
    }

    fn reply(status: &str, body: &str) -> String {
        format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
    }

    fn fetcher() -> Fetcher {
        Fetcher::new("Test", Duration::from_secs(5), Arc::default())
            .expect("a client")
            .with_backoff(Duration::from_millis(1))
    }

    #[test]
    fn statuses_are_classified() {
        assert_eq!(classify(StatusCode::OK), Answer::Ok);
        assert_eq!(classify(StatusCode::SERVICE_UNAVAILABLE), Answer::Transient);
        assert_eq!(
            classify(StatusCode::INTERNAL_SERVER_ERROR),
            Answer::Transient
        );
        assert_eq!(classify(StatusCode::TOO_MANY_REQUESTS), Answer::Transient);
        assert_eq!(classify(StatusCode::NOT_FOUND), Answer::Permanent);
        assert_eq!(classify(StatusCode::FORBIDDEN), Answer::Permanent);
    }

    #[test]
    fn a_transient_failure_is_retried_and_progress_is_reported() {
        let (url, server) = serve(vec![
            reply("503 Service Unavailable", ""),
            reply("200 OK", "positions"),
        ]);
        let mut seen = Vec::new();
        let body = fetcher()
            .get(&url, &mut |done, total| seen.push((done, total)))
            .expect("read");
        assert_eq!(body, b"positions");
        assert_eq!(seen.last(), Some(&(9, Some(9))));
        assert_eq!(server.join().expect("server"), 2);
    }

    #[test]
    fn retries_are_bounded_and_end_unavailable() {
        let (url, server) = serve(vec![reply("500 Internal Server Error", ""); 4]);
        let err = fetcher().get(&url, &mut |_, _| {}).expect_err("gives up");
        assert!(matches!(err, TrackerError::Unavailable { .. }), "{err:?}");
        assert_eq!(server.join().expect("server"), 4);
    }

    #[test]
    fn a_permanent_failure_is_not_retried() {
        let (url, server) = serve(vec![reply("404 Not Found", "")]);
        let err = fetcher().get(&url, &mut |_, _| {}).expect_err("refused");
        assert!(matches!(err, TrackerError::Network(_)), "{err:?}");
        assert_eq!(server.join().expect("server"), 1);
    }

    #[test]
    fn an_oversized_body_is_refused_unread() {
        let huge = format!(
            "HTTP/1.1 200 OK\r\nContent-Length: {}\r\nConnection: close\r\n\r\nxx",
            MAX_BODY_BYTES + 1
        );
        let (url, server) = serve(vec![huge]);
        let err = fetcher().get(&url, &mut |_, _| {}).expect_err("refused");
        assert!(err.to_string().contains("limit"), "{err}");
        assert_eq!(server.join().expect("server"), 1);
    }

    /// A redirect is followed on the same host; one to a host off the
    /// allow-list is refused at once, not requested and not retried.
    #[test]
    fn redirects_off_the_allow_list_are_refused() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let port = listener.local_addr().expect("an address").port();
        let server = std::thread::spawn(move || {
            let mut served = 0;
            for _ in 0..3 {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                let request = String::from_utf8_lossy(&buf[..n]).into_owned();
                let path = request.split_whitespace().nth(1).unwrap_or("").to_owned();
                let location = match path.as_str() {
                    "/same" => Some(format!("http://127.0.0.1:{port}/target")),
                    "/away" => Some("https://yb.tl.evil.invalid/x".to_owned()),
                    _ => None,
                };
                let reply = match location {
                    Some(to) => format!(
                        "HTTP/1.1 302 Found\r\nLocation: {to}\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"
                    ),
                    None => reply("200 OK", "ok"),
                };
                let _ = stream.write_all(reply.as_bytes());
                served += 1;
            }
            served
        });
        let fetcher = fetcher();
        let body = fetcher
            .get(&format!("http://127.0.0.1:{port}/same"), &mut |_, _| {})
            .expect("followed");
        assert_eq!(body, b"ok");
        let err = fetcher
            .get(&format!("http://127.0.0.1:{port}/away"), &mut |_, _| {})
            .expect_err("refused");
        assert!(matches!(err, TrackerError::Network(_)), "{err:?}");
        assert!(err.to_string().contains("not an allowed host"), "{err}");
        assert_eq!(server.join().expect("server"), 3, "no retry of the refusal");
    }

    /// A cancel ends a retry pause (here 30 s) at once.
    #[test]
    fn a_cancel_cuts_a_retry_pause_short() {
        let (url, _server) = serve(vec![reply("503 Service Unavailable", ""); 4]);
        let flag = Arc::new(AtomicBool::new(false));
        let fetcher = Fetcher::new("Test", Duration::from_secs(5), Arc::clone(&flag))
            .expect("a client")
            .with_backoff(Duration::from_secs(30));
        let canceller = {
            let flag = Arc::clone(&flag);
            std::thread::spawn(move || {
                std::thread::sleep(Duration::from_millis(300));
                flag.store(true, Ordering::SeqCst);
            })
        };
        let start = Instant::now();
        let err = fetcher.get(&url, &mut |_, _| {}).expect_err("cancelled");
        assert!(matches!(err, TrackerError::Cancelled), "{err:?}");
        assert!(start.elapsed() < Duration::from_secs(5));
        canceller.join().expect("canceller");
    }
}
