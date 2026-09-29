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
    /// A failure worth reporting at once. `status` is the response's status
    /// when there was one (so [`TrackerError::Http`] can carry it), absent
    /// for a failure with no response (a refused redirect, an oversized
    /// body) which becomes a plain [`TrackerError::Network`].
    Permanent {
        status: Option<u16>,
        why: String,
    },
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
                Err(Failure::Permanent {
                    status: Some(status),
                    why,
                }) => {
                    return Err(TrackerError::Http { status, why });
                }
                Err(Failure::Permanent { status: None, why }) => {
                    return Err(TrackerError::Network(why));
                }
            }
        }
    }

    fn attempt(
        &self,
        url: &str,
        timeout: Option<Duration>,
        progress: &mut dyn FnMut(u64, Option<u64>),
    ) -> std::result::Result<Vec<u8>, Failure> {
        // Compressed where the tracker offers it: YellowBrick's RaceSetup
        // and Blue Water Tracks' race JSON are about a tenth the size
        // gzipped (M14b). Decompressed here, not by reqwest, so progress
        // counts the bytes actually transferred against their announced
        // length, and so `pe-env`'s client (which shares reqwest's
        // features) is unchanged.
        let mut request = self
            .client
            .get(url)
            .header(reqwest::header::ACCEPT_ENCODING, "gzip");
        if let Some(timeout) = timeout {
            request = request.timeout(timeout);
        }
        let response = request.send().map_err(|e| {
            let why = format!("{} could not be reached: {}", self.tracker, chain(&e));
            if transient_error(&e) {
                Failure::Transient(why)
            } else {
                Failure::Permanent { status: None, why }
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
                return Err(Failure::Permanent {
                    status: Some(status.as_u16()),
                    why: format!("{} answered {status} for {url}", self.tracker),
                });
            }
        }
        let gzip = response
            .headers()
            .get(reqwest::header::CONTENT_ENCODING)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|v| v.trim().eq_ignore_ascii_case("gzip"));
        let total = response.content_length();
        if total.is_some_and(|n| n > MAX_BODY_BYTES) {
            return Err(Failure::Permanent {
                status: None,
                why: format!(
                    "{} offered {} bytes for {url}, more than the {MAX_BODY_BYTES}-byte limit",
                    self.tracker,
                    total.unwrap_or_default()
                ),
            });
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
                return Err(Failure::Permanent {
                    status: None,
                    why: format!(
                        "{}'s answer for {url} is larger than the {MAX_BODY_BYTES}-byte limit",
                        self.tracker
                    ),
                });
            }
            progress(body.len() as u64, total);
        }
        // Complete, whether or not the server announced a length.
        progress(body.len() as u64, Some(body.len() as u64));
        if gzip {
            return self.gunzip(&body, url);
        }
        Ok(body)
    }

    /// A gzipped body, inflated under the same cap as a plain one.
    fn gunzip(&self, body: &[u8], url: &str) -> std::result::Result<Vec<u8>, Failure> {
        let mut out = Vec::with_capacity(body.len().saturating_mul(4));
        flate2::read::GzDecoder::new(body)
            .take(MAX_BODY_BYTES + 1)
            .read_to_end(&mut out)
            .map_err(|e| {
                Failure::Transient(format!(
                    "{}'s compressed answer for {url} did not inflate: {e}",
                    self.tracker
                ))
            })?;
        if out.len() as u64 > MAX_BODY_BYTES {
            return Err(Failure::Permanent {
                status: None,
                why: format!(
                    "{}'s answer for {url} is larger than the {MAX_BODY_BYTES}-byte limit",
                    self.tracker
                ),
            });
        }
        Ok(out)
    }

    /// Starts a GET of `url` on its own thread, then `then` on the body
    /// there too (a decoder), so it runs while this thread reads something
    /// else: YellowBrick's positions download while its RaceSetup is read,
    /// Geovoile's tracks and reports while its config is (M14b). The
    /// thread stops with this fetcher's cancel; an answer nobody waits for
    /// any more is dropped.
    ///
    /// # Errors
    /// [`TrackerError::Network`] if no thread can be started.
    pub fn spawn<T: Send + 'static>(
        &self,
        url: String,
        timeout: Option<Duration>,
        then: impl FnOnce(Vec<u8>) -> Result<T> + Send + 'static,
    ) -> Result<Pending<T>> {
        let (tx, rx) = std::sync::mpsc::channel();
        let fetcher = self.clone();
        std::thread::Builder::new()
            .name("tracker-get".to_owned())
            .spawn(move || {
                let progress_tx = tx.clone();
                let result = fetcher
                    .read(&url, timeout, &mut |bytes, total| {
                        let _ = progress_tx.send(Message::Progress(bytes, total));
                    })
                    .and_then(then);
                let _ = tx.send(Message::Done(result));
            })
            .map_err(|e| TrackerError::Network(format!("no download thread: {e}")))?;
        Ok(Pending {
            rx,
            fetcher: self.clone(),
        })
    }
}

enum Message<T> {
    Progress(u64, Option<u64>),
    Done(Result<T>),
}

/// A GET started by [`Fetcher::spawn`].
#[derive(Debug)]
pub struct Pending<T> {
    rx: std::sync::mpsc::Receiver<Message<T>>,
    fetcher: Fetcher,
}

impl<T> std::fmt::Debug for Message<T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Progress(bytes, total) => write!(f, "Progress({bytes}, {total:?})"),
            Self::Done(_) => write!(f, "Done"),
        }
    }
}

impl<T> Pending<T> {
    /// Waits for the answer, passing on its progress; returns at once on
    /// cancel.
    ///
    /// # Errors
    /// Whatever the GET or `then` gave; [`TrackerError::Cancelled`].
    pub fn wait(self, progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<T> {
        loop {
            self.fetcher.check()?;
            match self.rx.recv_timeout(PAUSE_SLICE) {
                Ok(Message::Progress(bytes, total)) => progress(bytes, total),
                Ok(Message::Done(result)) => return result,
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => {
                    return Err(TrackerError::Network("the download stopped".to_owned()));
                }
            }
        }
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

    /// A gzipped answer (asked for with `Accept-Encoding: gzip`) is
    /// inflated, and a corrupt one is an error, not a partial body.
    #[test]
    fn gzipped_answers_are_inflated() {
        use flate2::write::GzEncoder;
        let text = "RaceSetup ".repeat(1000);
        let mut enc = GzEncoder::new(Vec::new(), flate2::Compression::default());
        enc.write_all(text.as_bytes()).expect("compresses");
        let gz = enc.finish().expect("compresses");
        let head = |len: usize| {
            format!(
                "HTTP/1.1 200 OK\r\nContent-Encoding: gzip\r\nContent-Length: {len}\r\nConnection: close\r\n\r\n"
            )
        };
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let addr = listener.local_addr().expect("an address");
        let bodies = vec![gz.clone(), gz[..gz.len() / 2].to_vec()];
        std::thread::spawn(move || {
            for body in bodies.into_iter().chain(std::iter::repeat_n(Vec::new(), 8)) {
                let Ok((mut stream, _)) = listener.accept() else {
                    break;
                };
                let mut buf = [0u8; 4096];
                let n = stream.read(&mut buf).unwrap_or(0);
                assert!(
                    String::from_utf8_lossy(&buf[..n])
                        .to_ascii_lowercase()
                        .contains("accept-encoding: gzip")
                );
                let _ = stream.write_all(head(body.len()).as_bytes());
                let _ = stream.write_all(&body);
            }
        });
        let url = format!("http://127.0.0.1:{}/x", addr.port());
        let mut last = (0, None);
        let body = fetcher()
            .get(&url, &mut |b, t| last = (b, t))
            .expect("inflates");
        assert_eq!(body, text.as_bytes());
        assert_eq!(
            last,
            (gz.len() as u64, Some(gz.len() as u64)),
            "progress counts the bytes sent"
        );
        assert!(fetcher().get(&url, &mut |_, _| {}).is_err(), "a cut gzip");
    }

    /// A GET on its own thread passes its progress and answer on; a cancel
    /// ends the wait at once.
    #[test]
    fn a_spawned_get_answers_and_cancels() {
        let (url, _) = serve(vec![reply("200 OK", "hello")]);
        let pending = fetcher()
            .spawn(url, None, |bytes| Ok(bytes.len()))
            .expect("spawns");
        let mut seen = 0;
        assert_eq!(pending.wait(&mut |b, _| seen = b).expect("answers"), 5);
        assert_eq!(seen, 5);
        let cancel = Arc::new(AtomicBool::new(false));
        let quiet = std::net::TcpListener::bind("127.0.0.1:0").expect("binds");
        let url = format!(
            "http://127.0.0.1:{}/x",
            quiet.local_addr().expect("addr").port()
        );
        let fetcher =
            Fetcher::new("Test", Duration::from_secs(30), Arc::clone(&cancel)).expect("a client");
        let pending = fetcher.spawn(url, None, Ok).expect("spawns");
        cancel.store(true, Ordering::SeqCst);
        let started = Instant::now();
        assert!(matches!(
            pending.wait(&mut |_, _| {}),
            Err(TrackerError::Cancelled)
        ));
        assert!(started.elapsed() < Duration::from_secs(2));
        drop(quiet);
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

    /// A permanent status is not retried, and is kept on the error so a
    /// caller can match a specific one (a 404) without matching the message
    /// text (CLAUDE.md "Adding a tracker").
    #[test]
    fn a_permanent_failure_is_not_retried_and_keeps_its_status() {
        let (url, server) = serve(vec![reply("404 Not Found", "")]);
        let err = fetcher().get(&url, &mut |_, _| {}).expect_err("refused");
        assert!(
            matches!(err, TrackerError::Http { status: 404, .. }),
            "{err:?}"
        );
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
