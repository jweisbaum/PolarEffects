#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Block reads over HTTP (M14e), against a real chunk served by a local
//! server that honours `Range` exactly as Google Cloud Storage does: only
//! the chunk's first bytes and the blocks holding the wanted rows cross the
//! wire, and every value equals the whole chunk decoded.
//!
//! # The fixture
//!
//! `fixtures/arco-swh-chunk/1100000.0.0` is ARCO-ERA5's significant wave
//! height at 2025-06-27T08Z (hour 1,100,000 since 1900), 1,735,217 bytes
//! exactly as served: a blosc container of eight blocks (block offsets 48,
//! 74,538, 248,354, 489,909, 796,482, 1,106,287, 1,446,896, 1,731,778). The
//! metadata and axes around it are written here: the real `.zarray`
//! (shape cut to end at that hour), latitude 90 to −90 and longitude 0 to
//! 359.75 by 0.25°, and a time axis of two-value chunks with only its first
//! and last chunk present. The reference values were decoded from the same
//! bytes with numcodecs 0.12 (c-blosc), independently of `blosc.rs`
//! (`fixtures/arco-swh-chunk/reference.py`).

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use pe_env::BlockCache;
use pe_env::blosc;
use pe_env::dataset::{OpenVariable, vars};
use pe_env::store::open_http;

const ARRAY: &str = "significant_height_of_combined_wind_waves_and_swell";
const CHUNK: &str = "significant_height_of_combined_wind_waves_and_swell/1100000.0.0";
/// Hour 1,100,000 since 1900-01-01.
const T: i64 = -2_208_988_800 + 1_100_000 * 3600;
/// The chunk's block offsets and size, from its header.
const STARTS: [u64; 8] = [
    48, 74_538, 248_354, 489_909, 796_482, 1_106_287, 1_446_896, 1_731_778,
];
const CBYTES: u64 = 1_735_217;

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/arco-swh-chunk/1100000.0.0")
}

/// Writes the store around the recorded chunk.
fn write_store(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
    let put = |path: &str, bytes: &[u8]| {
        let file = dir.join(path);
        std::fs::create_dir_all(file.parent().unwrap()).unwrap();
        std::fs::write(file, bytes).unwrap();
    };
    put(".zgroup", br#"{"zarr_format": 2}"#);
    put(".zattrs", b"{}");
    let axis = |name: &str, values: Vec<f32>| {
        let meta = serde_json::json!({
            "zarr_format": 2, "shape": [values.len()], "chunks": [values.len()],
            "dtype": "<f4", "compressor": null, "filters": null, "order": "C",
            "fill_value": "NaN"
        });
        put(&format!("{name}/.zarray"), meta.to_string().as_bytes());
        put(
            &format!("{name}/.zattrs"),
            serde_json::json!({ "_ARRAY_DIMENSIONS": [name] })
                .to_string()
                .as_bytes(),
        );
        let bytes: Vec<u8> = values.iter().flat_map(|v| v.to_le_bytes()).collect();
        put(&format!("{name}/0"), &bytes);
    };
    axis(
        "latitude",
        (0..721).map(|i| 90.0 - 0.25 * i as f32).collect(),
    );
    axis("longitude", (0..1440).map(|j| 0.25 * j as f32).collect());
    let steps = 1_100_001u64;
    let meta = serde_json::json!({
        "zarr_format": 2, "shape": [steps], "chunks": [2], "dtype": "<i8",
        "compressor": null, "filters": null, "order": "C", "fill_value": null
    });
    put("time/.zarray", meta.to_string().as_bytes());
    put(
        "time/.zattrs",
        br#"{"_ARRAY_DIMENSIONS": ["time"], "units": "hours since 1900-01-01 00:00:00"}"#,
    );
    let pair = |a: i64, b: i64| [a.to_le_bytes(), b.to_le_bytes()].concat();
    put("time/0", &pair(0, 1));
    put("time/550000", &pair(1_100_000, 1_100_001));
    let meta = serde_json::json!({
        "zarr_format": 2, "shape": [steps, 721, 1440], "chunks": [1, 721, 1440],
        "dtype": "<f4", "fill_value": "NaN", "filters": null, "order": "C",
        "compressor": {"blocksize": 0, "clevel": 5, "cname": "lz4", "id": "blosc", "shuffle": 1}
    });
    put(&format!("{ARRAY}/.zarray"), meta.to_string().as_bytes());
    put(
        &format!("{ARRAY}/.zattrs"),
        br#"{"_ARRAY_DIMENSIONS": ["time", "latitude", "longitude"], "units": "m"}"#,
    );
    put(CHUNK, &std::fs::read(fixture()).unwrap());
}

/// How the test server answers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// As Google Cloud Storage does.
    Honour,
    /// 200 with the whole object whatever the range.
    Ignore,
    /// Heads as usual, but 404 for any range past the start: a chunk gone
    /// between its header and its blocks.
    NoBlocks,
}

/// One request the server answered: the key, the range asked for (if
/// any) and the body bytes sent.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Served {
    key: String,
    range: Option<(u64, u64)>,
    bytes: u64,
}

/// A local HTTP server over `dir`: 206 with exactly the bytes of a
/// `Range: bytes=a-b` request (as Google Cloud Storage answers), 200 with
/// the whole object otherwise or always when `ignore_ranges`, 404 for a
/// missing key. Every answer is logged.
struct Server {
    url: String,
    log: Arc<Mutex<Vec<Served>>>,
    stop: Arc<AtomicBool>,
}

impl Server {
    fn start(dir: PathBuf, mode: Mode) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let log = Arc::new(Mutex::new(Vec::new()));
        let stop = Arc::new(AtomicBool::new(false));
        let (log2, stop2) = (Arc::clone(&log), Arc::clone(&stop));
        std::thread::spawn(move || {
            while !stop2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (dir, log) = (dir.clone(), Arc::clone(&log2));
                        std::thread::spawn(move || serve(stream, &dir, mode, &log));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self {
            url: format!("http://127.0.0.1:{port}/store"),
            log,
            stop,
        }
    }

    /// The requests for the chunk.
    fn chunk_requests(&self) -> Vec<Served> {
        self.log
            .lock()
            .unwrap()
            .iter()
            .filter(|s| s.key == CHUNK)
            .cloned()
            .collect()
    }

    fn clear(&self) {
        self.log.lock().unwrap().clear();
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve(mut stream: std::net::TcpStream, dir: &Path, mode: Mode, log: &Mutex<Vec<Served>>) {
    stream.set_nonblocking(false).unwrap();
    let mut request = Vec::new();
    let mut buf = [0u8; 4096];
    while !request.windows(4).any(|w| w == b"\r\n\r\n") {
        match stream.read(&mut buf) {
            Ok(0) | Err(_) => return,
            Ok(n) => request.extend_from_slice(&buf[..n]),
        }
    }
    let text = String::from_utf8_lossy(&request).into_owned();
    let path = text.split_whitespace().nth(1).unwrap_or("/");
    let key = path.trim_start_matches("/store/").to_owned();
    let range = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("range") {
            return None;
        }
        let spec = value.trim().strip_prefix("bytes=")?;
        let (a, b) = spec.split_once('-')?;
        Some((a.parse::<u64>().ok()?, b.parse::<u64>().ok()?))
    });
    let gone = mode == Mode::NoBlocks && range.is_some_and(|(a, _)| a > 0);
    let found = if gone {
        Err(())
    } else {
        std::fs::read(dir.join(&key)).map_err(|_| ())
    };
    let Ok(body) = found else {
        log.lock().unwrap().push(Served {
            key,
            range,
            bytes: 0,
        });
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let (status, part, extra) = match range {
        Some((a, b)) if mode != Mode::Ignore && (a as usize) < body.len() => {
            let end = (b as usize + 1).min(body.len());
            (
                "206 Partial Content",
                &body[a as usize..end],
                format!("Content-Range: bytes {a}-{}/{}\r\n", end - 1, body.len()),
            )
        }
        _ => ("200 OK", &body[..], String::new()),
    };
    log.lock().unwrap().push(Served {
        key,
        range,
        bytes: part.len() as u64,
    });
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        part.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(part);
}

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pe-ranges-{label}-{}", std::process::id()))
}

fn open(server: &Server) -> (OpenVariable, Arc<BlockCache>) {
    let store = open_http(&server.url, Duration::from_secs(10)).unwrap();
    let memory = BlockCache::new(64 << 20);
    let var = OpenVariable::open(&store, vars::ARCO_SWH)
        .unwrap()
        .with_memory(Arc::clone(&memory));
    (var, memory)
}

/// The value the whole chunk gives, decoded in one piece and interpolated
/// with the same stencil: what a block read must equal exactly.
fn whole_chunk_value(var: &OpenVariable, lat: f64, lon: f64) -> Option<f64> {
    let bytes = std::fs::read(fixture()).unwrap();
    let decoded = blosc::decompress(&bytes, blosc::Expected::Exactly(721 * 1440 * 4)).unwrap();
    let at = |r: usize, c: usize| {
        let i = (r * 1440 + c) * 4;
        f32::from_le_bytes(decoded[i..i + 4].try_into().unwrap())
    };
    let s = var.grid().stencil(lat, lon).unwrap();
    s.interpolate([
        at(s.lat[0], s.lon[0]),
        at(s.lat[0], s.lon[1]),
        at(s.lat[1], s.lon[0]),
        at(s.lat[1], s.lon[1]),
    ])
}

/// numcodecs' values (`reference.py`): position, and the value or none.
const REFERENCE: [(f64, f64, Option<f64>); 6] = [
    (50.1, -4.9, Some(1.104_234_066_009_521_3)),
    (44.375, 13.1, Some(0.514_108_133_316_04)),
    (90.0, 10.0, None),
    (-10.0, 359.9, Some(2.291_618_919_372_556_5)),
    (-60.1, -40.0, Some(2.318_238_401_412_967_3)),
    (-89.9, 20.0, None),
];

/// The acceptance test: one position in the western Channel (block 1)
/// costs the 64-byte head and block 1 only — 173,880 bytes of 1,735,217 —
/// and the value is the whole chunk's and numcodecs'.
#[test]
fn only_the_head_and_the_needed_block_are_requested() {
    let dir = temp("one-block");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::Honour);
    let (var, memory) = open(&server);
    assert!(var.reads_blocks());
    server.clear();
    let got = var.sample(T, 50.1, -4.9).unwrap();
    assert_eq!(got, whole_chunk_value(&var, 50.1, -4.9));
    assert!(
        (got.unwrap() - 1.104_234_066_009_521_3).abs() < 1e-6,
        "{got:?}"
    );
    assert_eq!(
        server.chunk_requests(),
        vec![
            Served {
                key: CHUNK.to_owned(),
                range: Some((0, 63)),
                bytes: 64,
            },
            Served {
                key: CHUNK.to_owned(),
                range: Some((STARTS[1], STARTS[2] - 1)),
                bytes: STARTS[2] - STARTS[1],
            },
        ]
    );
    let sent: u64 = server.chunk_requests().iter().map(|s| s.bytes).sum();
    assert_eq!(sent, 64 + 173_816);
    assert!(sent * 9 < CBYTES, "about a tenth of the chunk");
    // Read again (the second boat): nothing more crosses the wire.
    server.clear();
    assert!(var.sample(T, 50.2, -5.0).unwrap().is_some());
    assert!(server.chunk_requests().is_empty());
    assert!(memory.size() < 200_000, "{}", memory.size());
    let _ = std::fs::remove_dir_all(dir);
}

/// A stencil across the boundary of blocks 1 and 2 (row 182 is split at
/// column 64), the poles, the 0/360 seam and the short final block: each
/// value equals the whole chunk's, and each block is fetched once, the two
/// adjacent ones in one request.
#[test]
fn every_position_equals_the_whole_chunk_and_blocks_are_fetched_once() {
    let dir = temp("all");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::Honour);
    let (var, _memory) = open(&server);
    server.clear();
    // Blocks 1 and 2 at once: one request for both, back to back.
    let straddle = var.sample(T, 44.375, 13.1).unwrap();
    let reqs = server.chunk_requests();
    assert_eq!(reqs.len(), 2, "{reqs:?}");
    assert_eq!(reqs[1].range, Some((STARTS[1], STARTS[3] - 1)));
    assert_eq!(straddle, whole_chunk_value(&var, 44.375, 13.1));
    for (lat, lon, reference) in REFERENCE {
        let got = var.sample(T, lat, lon).unwrap();
        assert_eq!(got, whole_chunk_value(&var, lat, lon), "{lat} {lon}");
        match (got, reference) {
            (Some(g), Some(r)) => assert!((g - r).abs() < 1e-6, "{lat} {lon}: {g} vs {r}"),
            (None, None) => {}
            other => panic!("{lat} {lon}: {other:?}"),
        }
    }
    // One head and each block at most once over all of it.
    let reqs = server.chunk_requests();
    let heads = reqs.iter().filter(|r| r.range == Some((0, 63))).count();
    assert_eq!(heads, 1, "{reqs:?}");
    let sent: u64 = reqs.iter().map(|s| s.bytes).sum();
    // Blocks 0 (90N), 1 and 2 (the Channel, the Adriatic), 4 (10S: row
    // 400), 6 (60S) and 7 (the south pole, the short final block).
    let blocks = [0usize, 1, 2, 4, 6, 7];
    let expected: u64 = 64
        + blocks
            .iter()
            .map(|&b| STARTS.get(b + 1).copied().unwrap_or(CBYTES) - STARTS[b])
            .sum::<u64>();
    assert_eq!(sent, expected, "{reqs:?}");
    assert!(
        reqs.iter().all(|r| r.range.is_some()),
        "never the whole chunk"
    );
    let _ = std::fs::remove_dir_all(dir);
}

/// A server that ignores `Range` answers 200 with the whole chunk: it is
/// used whole, once, and the values are the same.
#[test]
fn a_server_that_ignores_ranges_is_read_whole_once() {
    let dir = temp("ignore");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::Ignore);
    let (var, _memory) = open(&server);
    server.clear();
    for (lat, lon, _) in REFERENCE {
        let got = var.sample(T, lat, lon).unwrap();
        assert_eq!(got, whole_chunk_value(&var, lat, lon), "{lat} {lon}");
    }
    let reqs = server.chunk_requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(reqs[0].bytes, CBYTES);
    let _ = std::fs::remove_dir_all(dir);
}

/// An hour the archive does not have (404) is missing, asked once.
#[test]
fn a_missing_chunk_is_no_data_and_asked_once() {
    let dir = temp("missing");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::Honour);
    let (var, _memory) = open(&server);
    let hour_before = T - 3600;
    server.clear();
    assert_eq!(var.sample(hour_before, 50.1, -4.9).unwrap(), None);
    assert_eq!(var.sample(hour_before, 44.0, 13.0).unwrap(), None);
    let asked = server
        .log
        .lock()
        .unwrap()
        .iter()
        .filter(|s| s.key.ends_with("1099999.0.0"))
        .count();
    assert_eq!(asked, 1);
    let _ = std::fs::remove_dir_all(dir);
}

/// Round 1 of review: values are read from the bytes fetched, never back
/// through the size-bounded memory. With a memory too small to hold one
/// block, and several readers at once, every value is still the whole
/// chunk's — none is lost as missing.
#[test]
fn a_memory_too_small_for_a_block_still_gives_exact_values() {
    let dir = temp("tiny-memory");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::Honour);
    let store = open_http(&server.url, Duration::from_secs(10)).unwrap();
    let memory = BlockCache::new(100);
    let var = OpenVariable::open(&store, vars::ARCO_SWH)
        .unwrap()
        .with_concurrency(8)
        .with_memory(Arc::clone(&memory));
    std::thread::scope(|scope| {
        for _ in 0..4 {
            scope.spawn(|| {
                for (lat, lon, reference) in REFERENCE {
                    let got = var.sample(T, lat, lon).unwrap();
                    assert_eq!(got, whole_chunk_value(&var, lat, lon), "{lat} {lon}");
                    assert_eq!(got.is_some(), reference.is_some(), "{lat} {lon}");
                }
            });
        }
    });
    assert_eq!(memory.size(), 0, "nothing fitted, and nothing was needed");
    let _ = std::fs::remove_dir_all(dir);
}

/// A chunk whose header is served but whose blocks then 404 fails the
/// read, and its header is forgotten, rather than reading as "fetched, no
/// data" that would never be asked for again.
#[test]
fn blocks_gone_after_their_header_fail_the_read() {
    let dir = temp("no-blocks");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::NoBlocks);
    let (var, memory) = open(&server);
    let err = var.sample(T, 50.1, -4.9).expect_err("must not be missing");
    assert!(err.to_string().contains("no longer has"), "{err}");
    assert!(!memory.contains("arco-era5", CHUNK, pe_env::memory::Part::Head));
    let _ = std::fs::remove_dir_all(dir);
}

/// Heads read ahead, side by side (review round 1): afterwards a sample
/// needs one request, its block.
#[test]
fn heads_read_ahead_leave_one_request_per_chunk() {
    let dir = temp("prefetch");
    write_store(&dir);
    let server = Server::start(dir.clone(), Mode::Honour);
    let (var, _memory) = open(&server);
    server.clear();
    let cells = pe_env::dataset::corner_cells(1_100_000, &var.grid().stencil(50.1, -4.9).unwrap());
    let read =
        pe_env::dataset::prefetch_heads(&[(&var, &cells[..])], &AtomicBool::new(false), 4).unwrap();
    assert_eq!(read, 1);
    assert_eq!(server.chunk_requests().len(), 1);
    assert_eq!(server.chunk_requests()[0].range, Some((0, 63)));
    server.clear();
    assert!(var.sample(T, 50.1, -4.9).unwrap().is_some());
    let reqs = server.chunk_requests();
    assert_eq!(reqs.len(), 1, "{reqs:?}");
    assert_eq!(reqs[0].range, Some((STARTS[1], STARTS[2] - 1)));
    // Held heads are not read again.
    assert_eq!(
        pe_env::dataset::prefetch_heads(&[(&var, &cells[..])], &AtomicBool::new(false), 4).unwrap(),
        0
    );
    let _ = std::fs::remove_dir_all(dir);
}
