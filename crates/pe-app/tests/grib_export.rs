#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The reanalysis GRIB export end to end (spec.md 7.8): a track's area and
//! hours read through the real provider from archives served over HTTP by
//! a local server that honours `Range` as Google Cloud Storage does, and
//! written as a `.grib2` file.
//!
//! **The golden file pins the bytes** (invariant 5, CLAUDE.md "Changing
//! export output"): `crates/pe-grib/tests/golden/reanalysis.grib2`, whose
//! SHA-256 is pinned below. Every CI target must write the same bytes, and
//! CI reads the file with ecCodes (`tools/check-grib.sh`). Rewrite it with
//! `PE_BLESS=1 cargo test -p pe-app --test grib_export` only when the
//! output is meant to change, and say why in the commit.
//!
//! # The archives served
//!
//! - **Wind**: the `wb2-crop` fixture of `pe-env` — WeatherBench2's
//!   2020-07-27T12Z u10 and v10 as served, blosc/LZ4, cropped to 55–45N ×
//!   350–359.75E (see `pe-env/tests/fixtures.rs`). numcodecs (c-blosc,
//!   independent of this code base) decodes u = 9.382978439331055, v =
//!   5.924107074737549 at 50N 5W. The store has no 11Z or 13Z chunk: those
//!   hours are missing in the archive, and must be written as missing.
//! - **Waves**: an ARCO-ERA5 store written here, uncompressed, over 56–44N
//!   × 345–359.75E for 11Z–13Z: height = (lat − 40) / 10 + step / 10,
//!   and direction = lon (in 0–360) − 100, both missing north of 51N
//!   and east of 357E (land).
//! - **Current**: a GlobCurrent store written here, uncompressed, on its
//!   own grid (cell centres 44.125–55.875N × 9.875W–0.875E) for 11Z–13Z:
//!   u = lat / 100, v = lon / 100 + step / 1000, missing north of 52N and
//!   east of 3W (land). Both are linear, so the bilinear regridding onto
//!   the 0.25° nodes gives the formula exactly wherever all four corners
//!   are sea. The NW Shelf, IBI and merged tiers are not served (their
//!   boxes hold the track, so this also checks the chain falls through).

mod common;

use std::io::{Read, Write};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use common::TempRoot;
use pe_app::grib::{self, GribExportStatus, GribSink, Outcome, Plan};
use pe_env::dataset::Dataset;
use pe_env::{Access, Interval, Reanalysis};
use pe_grib::reader::{Decoded, decode_all};
use sha2::{Digest, Sha256};

/// The pinned SHA-256 of the golden export.
const GOLDEN_SHA256: &str = "74936de2536ed1221c8e8e13f5a78992191e9b7ce9308eda8c28a7969912135c";

/// 2020-07-27T11:00Z.
const T11: i64 = 1_595_847_600;

fn golden_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pe-grib/tests/golden/reanalysis.grib2")
}

fn wb2_fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../pe-env/tests/fixtures/wb2-crop")
}

// ------------------------------------------------------------- stores

fn put(path: &Path, bytes: &[u8]) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, bytes).unwrap();
}

fn f4(values: &[f32]) -> Vec<u8> {
    values.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// An array's value at a step, latitude and longitude.
type Field<'a> = &'a dyn Fn(usize, f32, f32) -> f32;

/// An uncompressed Zarr v2 store: axes and `(time, latitude, longitude)`
/// arrays, one chunk holding every step.
fn write_store(
    dir: &Path,
    epoch: &str,
    first_hour: i64,
    lats: &[f32],
    lons: &[f32],
    arrays: &[(&str, Field<'_>)],
) {
    let steps = 3usize;
    put(&dir.join(".zgroup"), br#"{"zarr_format": 2}"#);
    put(&dir.join(".zattrs"), b"{}");
    let meta = |shape: &[usize], dtype: &str| {
        serde_json::json!({
            "zarr_format": 2, "shape": shape, "chunks": shape, "dtype": dtype,
            "compressor": null, "filters": null, "order": "C",
            "fill_value": if dtype == "<f4" { serde_json::json!("NaN") } else { serde_json::Value::Null }
        })
        .to_string()
    };
    let dims = |names: &[&str]| serde_json::json!({ "_ARRAY_DIMENSIONS": names }).to_string();
    for (name, values) in [("latitude", lats), ("longitude", lons)] {
        put(
            &dir.join(name).join(".zarray"),
            meta(&[values.len()], "<f4").as_bytes(),
        );
        put(&dir.join(name).join(".zattrs"), dims(&[name]).as_bytes());
        put(&dir.join(name).join("0"), &f4(values));
    }
    put(&dir.join("time/.zarray"), meta(&[steps], "<i8").as_bytes());
    put(
        &dir.join("time/.zattrs"),
        serde_json::json!({ "_ARRAY_DIMENSIONS": ["time"], "units": format!("hours since {epoch}") })
            .to_string()
            .as_bytes(),
    );
    let hours: Vec<u8> = (0..steps as i64)
        .flat_map(|k| (first_hour + k).to_le_bytes())
        .collect();
    put(&dir.join("time/0"), &hours);
    for (name, value) in arrays {
        let d = dir.join(name);
        put(
            &d.join(".zarray"),
            meta(&[steps, lats.len(), lons.len()], "<f4").as_bytes(),
        );
        put(
            &d.join(".zattrs"),
            dims(&["time", "latitude", "longitude"]).as_bytes(),
        );
        let mut all = Vec::new();
        for step in 0..steps {
            for &lat in lats {
                for &lon in lons {
                    all.push(value(step, lat, lon));
                }
            }
        }
        put(&d.join("0.0.0"), &f4(&all));
    }
}

fn copy_dir(from: &Path, to: &Path) {
    for entry in std::fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            put(&target, &std::fs::read(entry.path()).unwrap());
        }
    }
}

/// Hours since `epoch` of 2020-07-27T11Z.
fn hours_since(epoch_unix: i64) -> i64 {
    (T11 - epoch_unix) / 3600
}

/// Every archive the export reads, under `root/<dataset id>`.
fn write_archives(root: &Path) {
    copy_dir(&wb2_fixture(), &root.join(Dataset::Wb2Era5Hourly.id()));
    let lats: Vec<f32> = (0..49).map(|i| 56.0 - 0.25 * i as f32).collect();
    let lons: Vec<f32> = (0..60).map(|j| 345.0 + 0.25 * j as f32).collect();
    let height = |step: usize, lat: f32, lon: f32| {
        if lat > 51.0 && lon > 357.0 {
            f32::NAN
        } else {
            (lat - 40.0) / 10.0 + step as f32 / 10.0
        }
    };
    let direction = |_: usize, lat: f32, lon: f32| {
        if lat > 51.0 && lon > 357.0 {
            f32::NAN
        } else {
            lon - 100.0
        }
    };
    write_store(
        &root.join(Dataset::ArcoEra5.id()),
        "1900-01-01",
        hours_since(-2_208_988_800),
        &lats,
        &lons,
        &[
            (
                "significant_height_of_combined_wind_waves_and_swell",
                &height,
            ),
            ("mean_wave_direction", &direction),
            ("mean_wave_period", &|_, _, _| 8.0),
        ],
    );
    let lats: Vec<f32> = (0..48).map(|i| 44.125 + 0.25 * i as f32).collect();
    let lons: Vec<f32> = (0..44).map(|j| -9.875 + 0.25 * j as f32).collect();
    let land = |lat: f32, lon: f32| lat > 52.0 && lon > -3.0;
    let u = |_: usize, lat: f32, lon: f32| {
        if land(lat, lon) {
            f32::NAN
        } else {
            lat / 100.0
        }
    };
    let v = |step: usize, lat: f32, lon: f32| {
        if land(lat, lon) {
            f32::NAN
        } else {
            lon / 100.0 + step as f32 / 1000.0
        }
    };
    write_store(
        &root.join(Dataset::GlobCurrentMy.id()),
        "1950-01-01",
        hours_since(-631_152_000),
        &lats,
        &lons,
        &[("uo", &u), ("vo", &v)],
    );
}

// ------------------------------------------------------------- server

/// A local server over `dir`: 206 with exactly the bytes of a `Range`
/// request, 200 with the whole object otherwise, 404 for a missing key.
struct Server {
    origin: String,
    requests: Arc<std::sync::Mutex<Vec<(String, bool)>>>,
    stop: Arc<AtomicBool>,
}

impl Server {
    fn start(dir: PathBuf) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        listener.set_nonblocking(true).unwrap();
        let stop = Arc::new(AtomicBool::new(false));
        let requests = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (stop2, log) = (Arc::clone(&stop), Arc::clone(&requests));
        std::thread::spawn(move || {
            while !stop2.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        let (dir, log) = (dir.clone(), Arc::clone(&log));
                        std::thread::spawn(move || serve(stream, &dir, &log));
                    }
                    Err(_) => std::thread::sleep(Duration::from_millis(2)),
                }
            }
        });
        Self {
            origin: format!("http://127.0.0.1:{port}"),
            requests,
            stop,
        }
    }
}

impl Drop for Server {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::SeqCst);
    }
}

fn serve(mut stream: std::net::TcpStream, dir: &Path, log: &std::sync::Mutex<Vec<(String, bool)>>) {
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
    let key = text
        .split_whitespace()
        .nth(1)
        .unwrap_or("/")
        .trim_start_matches('/')
        .to_owned();
    let range = text.lines().find_map(|line| {
        let (name, value) = line.split_once(':')?;
        if !name.eq_ignore_ascii_case("range") {
            return None;
        }
        let (a, b) = value.trim().strip_prefix("bytes=")?.split_once('-')?;
        Some((a.parse::<usize>().ok()?, b.parse::<usize>().ok()?))
    });
    log.lock().unwrap().push((key.clone(), range.is_some()));
    let Ok(body) = std::fs::read(dir.join(&key)) else {
        let _ = stream
            .write_all(b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        return;
    };
    let (status, part, extra) = match range {
        Some((a, b)) if a < body.len() => {
            let end = (b + 1).min(body.len());
            (
                "206 Partial Content",
                &body[a..end],
                format!("Content-Range: bytes {a}-{}/{}\r\n", end - 1, body.len()),
            )
        }
        _ => ("200 OK", &body[..], String::new()),
    };
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Length: {}\r\n{extra}Connection: close\r\n\r\n",
        part.len()
    );
    let _ = stream.write_all(head.as_bytes());
    let _ = stream.write_all(part);
}

fn provider(origin: &str) -> Reanalysis {
    let urls = [
        Dataset::Wb2Era5Hourly,
        Dataset::ArcoEra5,
        Dataset::GlobCurrentMy,
    ]
    .into_iter()
    .map(|d| (d, format!("{origin}/{}", d.id())))
    .collect();
    Reanalysis::new(
        Access::Urls {
            timeout: Duration::from_secs(10),
            urls,
        },
        4,
    )
}

// -------------------------------------------------------------- tests

/// Three fixes from 11:30 to 12:30Z off the Lizard: the area 48–52.75N ×
/// 7W–2W (20 rows, 21 columns), hours 11, 12 and 13Z.
fn plan(waves: bool, current: bool) -> Plan {
    let fixes = [
        (T11 + 1800, 50.0, -5.0),
        (T11 + 3600, 50.3, -4.6),
        (T11 + 5400, 50.6, -4.2),
    ];
    Plan::of(&fixes, waves, current, Interval::Hourly, false).unwrap()
}

fn tolerance(values: &[f32]) -> f32 {
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let (lo, hi) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| {
        (a.min(v), b.max(v))
    });
    ((hi - lo) / 65535.0).max(1e-6) * 1.5
}

/// The value at the node `(lat, lon)` of a decoded message.
fn at(m: &Decoded, lat: f64, lon: f64) -> f32 {
    let j = ((f64::from(m.la1) / 1e6 - lat) / 0.25).round() as usize;
    let west = f64::from(m.lo1) / 1e6;
    let i = ((lon.rem_euclid(360.0) - west).rem_euclid(360.0) / 0.25).round() as usize;
    m.values[j * m.ni as usize + i]
}

fn export_to(dir: &Path, name: &str, plan: &Plan) -> (Vec<u8>, grib::Summary) {
    let archives = dir.join("archives");
    if !archives.exists() {
        write_archives(&archives);
    }
    let server = Server::start(archives);
    let p = provider(&server.origin);
    let path = dir.join(name);
    let outcome = grib::export(
        &p,
        plan,
        &path,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    let Outcome::Done(summary) = outcome else {
        panic!("{outcome:?}");
    };
    // The wind chunk was read by its head and blocks, never whole.
    let requests = server.requests.lock().unwrap().clone();
    let wind: Vec<_> = requests
        .iter()
        .filter(|(k, _)| k.ends_with("539724.0.0"))
        .collect();
    assert!(
        !wind.is_empty() && wind.iter().all(|(_, ranged)| *ranged),
        "{wind:?}"
    );
    (std::fs::read(&path).unwrap(), summary)
}

/// The acceptance test: every message decodes, with the right times,
/// grid, parameters and values, and the bytes are the golden file's.
#[test]
fn a_track_exports_wind_waves_and_current_to_the_golden_bytes() {
    let root = TempRoot::new("grib-golden");
    let plan = plan(true, true);
    assert_eq!((plan.grid.ni, plan.grid.nj), (21, 20));
    let (bytes, summary) = export_to(&root.0, "race.grib2", &plan);
    assert_eq!(summary.messages, 18);
    assert_eq!(summary.bytes, bytes.len() as u64);
    // 11Z and 13Z have no wind chunk: 4 empty messages.
    assert_eq!(summary.empty_messages, 4);

    let decoded = decode_all(&bytes);
    assert_eq!(decoded.len(), 18);
    let order = [
        (0, 2, 2),
        (0, 2, 3),
        (10, 0, 3),
        (10, 0, 14),
        (10, 1, 2),
        (10, 1, 3),
    ];
    for (k, m) in decoded.iter().enumerate() {
        assert_eq!(
            (m.discipline, m.category, m.number),
            order[k % 6],
            "message {k}"
        );
        assert_eq!(m.forecast_hour, (k / 6) as u32, "message {k}");
        let r = m.reference_time;
        assert_eq!((r.year, r.month, r.day, r.hour), (2020, 7, 27, 11));
        assert_eq!((m.la1, m.la2), (52_750_000, 48_000_000));
        assert_eq!((m.lo1, m.lo2), (353_000_000, 358_000_000));
        assert_eq!((m.di, m.dj), (250_000, 250_000));
        // 16 bits, or none for an empty or constant field.
        assert!(
            m.bits == 16 || (m.bits == 0 && m.values.iter().all(|x| x.is_nan())),
            "message {k}"
        );
    }

    // Wind: numcodecs' value at 50N 5W at 12Z; nothing at 11Z and 13Z.
    let (u, v) = (&decoded[6], &decoded[7]);
    let tol = tolerance(&u.values);
    assert!(
        (at(u, 50.0, -5.0) - 9.382_978).abs() <= tol,
        "{}",
        at(u, 50.0, -5.0)
    );
    assert!((at(v, 50.0, -5.0) - 5.924_107).abs() <= tolerance(&v.values));
    for k in [0, 1, 12, 13] {
        assert!(decoded[k].values.iter().all(|x| x.is_nan()), "message {k}");
    }
    assert!(u.values.iter().all(|x| x.is_finite()), "wind has no land");

    // Waves: the formulas on the nodes, missing on land.
    for (step, base) in [(0usize, 0), (1, 6), (2, 12)] {
        let (hs, dir) = (&decoded[base + 2], &decoded[base + 3]);
        let tol = tolerance(&hs.values);
        for (lat, lon) in [(48.0, -7.0), (50.0, -5.0), (52.75, -3.25), (49.5, -2.0)] {
            let want = (lat as f32 - 40.0) / 10.0 + step as f32 / 10.0;
            assert!((at(hs, lat, lon) - want).abs() <= tol, "{lat} {lon}");
            let want_dir = (lon as f32).rem_euclid(360.0) - 100.0;
            assert!((at(dir, lat, lon) - want_dir).abs() <= tolerance(&dir.values));
        }
        assert!(at(hs, 52.0, -2.5).is_nan(), "land at 52N 2.5W");
        assert!(at(hs, 52.75, -2.0).is_nan());
        assert!(at(dir, 52.0, -2.5).is_nan(), "no direction on land");
        assert!(at(dir, 52.75, -2.0).is_nan());
    }

    // Current: bilinear from GlobCurrent's cell centres to the nodes;
    // exact for these linear fields where all four corners are sea.
    for (step, base) in [(0usize, 0), (1, 6), (2, 12)] {
        let (cu, cv) = (&decoded[base + 4], &decoded[base + 5]);
        for (lat, lon) in [(48.0, -7.0), (50.0, -5.0), (51.5, -2.0), (52.75, -3.5)] {
            let want_u = lat as f32 / 100.0;
            let want_v = lon as f32 / 100.0 + step as f32 / 1000.0;
            assert!(
                (at(cu, lat, lon) - want_u).abs() <= tolerance(&cu.values) + 1e-6,
                "{lat} {lon}"
            );
            assert!(
                (at(cv, lat, lon) - want_v).abs() <= tolerance(&cv.values) + 1e-6,
                "{lat} {lon}"
            );
        }
        // Every corner land.
        assert!(at(cu, 52.75, -2.0).is_nan());
        assert!(at(cv, 52.5, -2.5).is_nan());
    }

    if std::env::var("PE_BLESS").is_ok_and(|v| v == "1") {
        std::fs::create_dir_all(golden_path().parent().unwrap()).unwrap();
        std::fs::write(golden_path(), &bytes).unwrap();
    }
    let golden = std::fs::read(golden_path()).expect("the golden file");
    assert!(bytes == golden, "the export differs from the golden file");
    let sha = format!("{:x}", Sha256::digest(&bytes));
    assert_eq!(sha, GOLDEN_SHA256, "the pinned hash");
}

/// Wind alone is the first two messages of every hour, with no waves or
/// current read, and the same values as among the rest.
#[test]
fn wind_alone_writes_two_messages_an_hour() {
    let root = TempRoot::new("grib-wind");
    let (bytes, summary) = export_to(&root.0, "wind.grib2", &plan(false, false));
    assert_eq!(summary.messages, 6);
    let all = std::fs::read(golden_path()).unwrap();
    let full = decode_all(&all);
    let wind = decode_all(&bytes);
    for (k, m) in wind.iter().enumerate() {
        let same = &full[(k / 2) * 6 + k % 2];
        assert_eq!(m.values.len(), same.values.len());
        for (a, b) in m.values.iter().zip(&same.values) {
            assert!(a == b || (a.is_nan() && b.is_nan()));
        }
    }
}

/// A cancelled export writes nothing: what was at the path stays, and no
/// temporary file (the export's or the current's) is left.
#[test]
fn a_cancelled_export_leaves_the_path_as_it_was() {
    let root = TempRoot::new("grib-cancel");
    let archives = root.0.join("archives");
    write_archives(&archives);
    let server = Server::start(archives);
    let p = provider(&server.origin);
    let path = root.0.join("race.grib2");
    std::fs::write(&path, b"an earlier file").unwrap();
    let cancel = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&cancel);
    // Cancelled as soon as the first part is done.
    let outcome = grib::export(&p, &plan(true, true), &path, &cancel, &mut |f| {
        if f > 0.0 {
            flag.store(true, Ordering::SeqCst);
        }
    })
    .unwrap();
    assert_eq!(outcome, Outcome::Cancelled);
    assert_eq!(std::fs::read(&path).unwrap(), b"an earlier file");
    let left: Vec<_> = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

/// A panic mid-export (a bug) unwinds through the file and the spool,
/// which delete their temporaries; what was at the path stays (M17a). It
/// panics on the progress after the last hour's messages are written:
/// the current is still spooled and every message is in the temporary,
/// the commit not yet made.
#[test]
fn a_panic_mid_export_leaves_no_temporary() {
    let root = TempRoot::new("grib-panic-mid");
    let archives = root.0.join("archives");
    write_archives(&archives);
    let server = Server::start(archives);
    let p = provider(&server.origin);
    let path = root.0.join("race.grib2");
    std::fs::write(&path, b"an earlier file").unwrap();
    let temp = pe_grib::file::temp_path_for(&path);
    let spool = root.0.join("race.grib2.current.tmp");
    let mut seen = false;
    let unwound = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        grib::export(
            &p,
            &plan(true, true),
            &path,
            &Arc::new(AtomicBool::new(false)),
            &mut |f| {
                if f >= 1.0 {
                    seen = temp.exists() && spool.exists();
                    panic!("a bug mid-export");
                }
            },
        )
    }));
    assert!(unwound.is_err(), "the export panicked");
    assert!(seen, "with the temporary and the spool on disk");
    assert_eq!(std::fs::read(&path).unwrap(), b"an earlier file");
    let left: Vec<_> = std::fs::read_dir(&root.0)
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.contains(".tmp"))
        .collect();
    assert!(left.is_empty(), "{left:?}");
}

struct Quiet;

impl GribSink for Quiet {
    fn progress(&self, _status: &GribExportStatus) {}
}

/// A project holding one track off the Lizard, 11:30 to 12:30Z.
fn project_with_track(root: &TempRoot) -> (pe_app::commands::AppState, u64) {
    use pe_app::tracks::{self, TrackFileRequest};
    let app = root.state();
    pe_app::projects::create(&app, "Grib".to_owned(), None, false).unwrap();
    let features: Vec<String> = [(T11 + 1800, 50.0, -5.0), (T11 + 5400, 50.6, -4.2)]
        .iter()
        .map(|(t, lat, lon)| {
            format!(
                r#"{{"type":"Feature","geometry":{{"type":"Point","coordinates":[{lon},{lat}]}},"properties":{{"time":{t},"boat":"Alpha"}}}}"#
            )
        })
        .collect();
    let file = root.file("race.geojson");
    std::fs::write(
        &file,
        format!(
            r#"{{"type":"FeatureCollection","features":[{}]}}"#,
            features.join(",")
        ),
    )
    .unwrap();
    let imported = tracks::import(
        &app,
        &[TrackFileRequest {
            path: file,
            mapping: None,
            boats: None,
        }],
    )
    .unwrap();
    (app, imported.imported[0].source_id)
}

/// Through the app: the plan comes from the open project's track, the job
/// refuses a second export while one runs, and its status ends "done"
/// with the file's size.
#[test]
fn the_job_exports_a_project_track_once_at_a_time() {
    let root = TempRoot::new("grib-job");
    let (app, id) = project_with_track(&root);

    let (planned, label) = grib::plan_for(&app, id, "hourly", true, false).unwrap();
    assert_eq!(label, "Alpha");
    assert_eq!(planned.times, vec![T11, T11 + 3600, T11 + 7200]);
    assert!(grib::plan_for(&app, id, "weekly", true, false).is_err());

    let out = root.file("alpha.grib2");
    let (plan, cancel) = grib::begin(&app, id, &out, "hourly", true, false).unwrap();
    assert_eq!(app.grib_jobs.status().state, "running");
    assert!(
        grib::begin(&app, id, &out, "hourly", true, false).is_err(),
        "one at a time"
    );

    let archives = root.0.join("archives");
    write_archives(&archives);
    let server = Server::start(archives);
    let p = provider(&server.origin);
    let status = grib::run(&app, &p, &|| p.net_totals().1, &plan, &out, &cancel, &Quiet);
    assert_eq!(status.state, "done", "{status:?}");
    assert_eq!(status.messages, 12);
    assert!(status.downloaded_bytes > 0);
    assert_eq!(status.bytes, std::fs::metadata(&out).unwrap().len());
    assert_eq!(app.grib_jobs.status(), status);
    // Done, so another may start.
    assert!(grib::begin(&app, id, &out, "three_hourly", false, false).is_ok());
}

/// What a real track's GRIB export costs (plan.md M16): live, never in the
/// default suite (`#[ignore]`, `PE_TEST_LIVE=1`).
///
/// ```text
/// PE_TEST_LIVE=1 cargo test -p pe-app --test grib_export live -- --ignored --nocapture
/// ```
///
/// A Fastnet 2025 boat of about `PE_GRIB_DAYS` days (default 2) is
/// imported from YellowBrick and its reanalysis exported through the real
/// archives: wind and waves, hourly, and the current if `PE_GRIB_CURRENT=1`.
/// It prints the dialog's estimate, the bytes downloaded, the file's size
/// and the seconds, and decodes every message with the test reader. If
/// ecCodes' `grib_ls` is on the PATH, it must read the file too.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
#[allow(clippy::print_stdout, reason = "the figures are the point")]
fn live_a_fastnet_boat_exports_to_grib() {
    if std::env::var("PE_TEST_LIVE").as_deref() != Ok("1") {
        return;
    }
    let root = TempRoot::new("grib-live");
    let app = root.state();
    pe_app::projects::create(&app, "Grib".to_owned(), None, false).unwrap();
    let view = pe_app::trackers::download_listed(
        &app,
        Arc::new(pe_trackers::yellowbrick::YellowBrick::default()),
        "yb.tl/fastnet2025",
        true,
        |_| {},
        |_| {},
    )
    .unwrap();
    let days: i64 = std::env::var("PE_GRIB_DAYS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(2);
    let current = std::env::var("PE_GRIB_CURRENT").as_deref() == Ok("1");
    let boat = view
        .boats
        .iter()
        .filter(|b| b.fixes > 0)
        .filter_map(|b| Some((b.last? - b.first?, b)))
        .min_by_key(|(span, _)| (span - days * 86_400).abs())
        .map(|(_, b)| b.id.clone())
        .unwrap();
    let imported = pe_app::trackers::import_boats(
        &app,
        pe_core::track::Tracker::YellowBrick,
        &view.key,
        &[boat],
    )
    .unwrap();
    let id = imported.imported[0].source_id;
    let (plan, label) = grib::plan_for(&app, id, "hourly", true, current).unwrap();
    let provider = pe_app::env::provider(&app).unwrap();
    let preview = grib::preview_of(id, &label, &plan, Some(provider.memory()));
    let out = root.file("live.grib2");
    let (_, b0) = provider.net_totals();
    let start = std::time::Instant::now();
    let outcome = grib::export(
        provider.as_ref(),
        &plan,
        Path::new(&out),
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .unwrap();
    let seconds = start.elapsed().as_secs_f64();
    let (_, b1) = provider.net_totals();
    let Outcome::Done(summary) = outcome else {
        panic!("{outcome:?}");
    };
    let bytes = std::fs::read(&out).unwrap();
    let decoded = decode_all(&bytes);
    assert_eq!(decoded.len() as u64, plan.messages());
    let expected_download =
        preview.wind_bytes + preview.waves_bytes + if current { preview.current_bytes } else { 0 };
    println!(
        "M16 | {label}: {} × {} points, {} hours, {} messages | estimate {:.1} MB download \
         (wind {:.1}, waves {:.1}, current {:.1}), file ~{:.2} MB | downloaded {:.1} MB in \
         {seconds:.1} s | file {:.2} MB, {} empty messages",
        plan.grid.ni,
        plan.grid.nj,
        plan.times.len(),
        summary.messages,
        expected_download as f64 / 1e6,
        preview.wind_bytes as f64 / 1e6,
        preview.waves_bytes as f64 / 1e6,
        preview.current_bytes as f64 / 1e6,
        preview.file_bytes as f64 / 1e6,
        (b1 - b0) as f64 / 1e6,
        summary.bytes as f64 / 1e6,
        summary.empty_messages,
    );
    let listed = std::process::Command::new("grib_ls").arg(&out).output();
    match listed {
        Ok(result) => {
            assert!(result.status.success(), "grib_ls failed");
            let text = String::from_utf8_lossy(&result.stdout);
            let last = text
                .lines()
                .rev()
                .find(|l| l.contains("messages"))
                .unwrap_or("");
            println!("M16 | grib_ls: {last}");
        }
        Err(_) => println!("M16 | grib_ls is not installed; ecCodes did not check the file"),
    }
    std::fs::copy(&out, std::env::temp_dir().join("pe-grib-live.grib2")).unwrap();
}

/// A provider whose archive will not answer: prepare fails, and so does
/// every read; or, with `panics`, a read panics (a bug).
struct Broken {
    panics: bool,
}

impl pe_env::Provider for Broken {
    fn sample(
        &self,
        points: &[pe_env::Point],
        _options: &pe_env::Options,
        _cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<Vec<pe_env::EnvPoint>> {
        assert!(!self.panics, "a bug in a read");
        let _ = points;
        Err(pe_env::EnvError::Open("the archive is down".to_owned()))
    }

    fn prepare(
        &self,
        _points: &[pe_env::Point],
        _options: &pe_env::Options,
        _cancel: &Arc<AtomicBool>,
    ) -> pe_env::Result<()> {
        if self.panics {
            return Ok(());
        }
        Err(pe_env::EnvError::Open("the archive is down".to_owned()))
    }
}

/// Review round 1: a prepare that fails without a cancel is a failure,
/// not a cancel, and names why; nothing is written.
#[test]
fn a_failing_archive_fails_the_export_rather_than_cancelling_it() {
    let root = TempRoot::new("grib-broken");
    let path = root.0.join("race.grib2");
    let err = grib::export(
        &Broken { panics: false },
        &plan(true, false),
        &path,
        &Arc::new(AtomicBool::new(false)),
        &mut |_| {},
    )
    .expect_err("a failure");
    assert!(err.to_string().contains("the archive is down"), "{err}");
    assert!(!path.exists());
    assert!(!pe_grib::file::temp_path_for(&path).exists());
    // Cancelled while it failed: a cancel.
    let cancelled = grib::export(
        &Broken { panics: false },
        &plan(true, false),
        &path,
        &Arc::new(AtomicBool::new(true)),
        &mut |_| {},
    )
    .unwrap();
    assert_eq!(cancelled, Outcome::Cancelled);
}

/// Review round 1: an export thread that panics leaves the job "failed",
/// not "running" for ever, and no file.
#[test]
fn a_panicking_export_ends_failed() {
    let root = TempRoot::new("grib-panic");
    let (app, id) = project_with_track(&root);
    let out = root.file("alpha.grib2");
    let (plan, cancel) = grib::begin(&app, id, &out, "hourly", false, false).unwrap();
    let joined = std::thread::scope(|scope| {
        scope
            .spawn(|| {
                grib::run(
                    &app,
                    &Broken { panics: true },
                    &|| 0,
                    &plan,
                    &out,
                    &cancel,
                    &Quiet,
                )
            })
            .join()
    });
    assert!(joined.is_err(), "the read panicked");
    let status = app.grib_jobs.status();
    assert_eq!(status.state, "failed", "{status:?}");
    assert!(!Path::new(&out).exists());
    assert!(grib::begin(&app, id, &out, "hourly", false, false).is_ok());
}
