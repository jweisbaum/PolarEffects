#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::print_stdout,
    clippy::too_many_arguments,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Live reads from the real archives: the M3 reanalysis-cost spike, redone
//! for block reads (M14e).
//!
//! Never in the default suite: `#[ignore]` and gated on `PE_TEST_LIVE=1`.
//!
//! ```text
//! PE_TEST_LIVE=1 cargo test -p pe-env --test live -- --ignored --nocapture --test-threads=1
//! ```
//!
//! Each test prints requests, bytes and seconds, cold and from memory, and
//! the bytes the same reads would have cost as whole chunks (the sum of the
//! chunks' own sizes, from their headers), so the numbers in `plan.md` can
//! be reproduced. Everything together downloads well under 100 MB.

use std::sync::Arc;
use std::sync::atomic::AtomicBool;
use std::time::{Duration, Instant};

use pe_env::BlockCache;
use pe_env::dataset::{Dataset, OpenVariable, Variable, vars};
use pe_env::http::NetStats;
use pe_env::store::open_http;
use pe_env::time::{parse_utc, to_iso};

fn live() -> bool {
    std::env::var("PE_TEST_LIVE").is_ok_and(|v| v == "1")
}

const TIMEOUT: Duration = Duration::from_secs(120);

struct Run {
    label: String,
    steps: usize,
    requests: u64,
    bytes: u64,
    seconds: f64,
    whole: u64,
}

impl Run {
    fn print(&self) {
        println!(
            "M14e | {:<46} | steps {:>3} | req {:>4} | {:>7.3} MB | {:>6.2} s | whole chunks would be {:>7.2} MB",
            self.label,
            self.steps,
            self.requests,
            self.bytes as f64 / 1e6,
            self.seconds,
            self.whole as f64 / 1e6,
        );
    }
}

/// Reads `steps` of `var` at one point, measuring the network.
fn measure(
    label: &str,
    variable: &OpenVariable,
    net: &NetStats,
    memory: &BlockCache,
    steps: &[u64],
    lat: f64,
    lon: f64,
    concurrency: usize,
) -> (Run, Vec<Option<f64>>) {
    let stencil = variable.grid().stencil(lat, lon).expect("inside the grid");
    let (r0, b0, _) = net.snapshot();
    let w0 = memory.whole_chunk_bytes();
    let start = Instant::now();
    let corners = variable
        .stencil_at_steps(steps, &stencil, concurrency)
        .expect("reads");
    let seconds = start.elapsed().as_secs_f64();
    let (r1, b1, _) = net.snapshot();
    let values = corners.iter().map(|c| stencil.interpolate(*c)).collect();
    (
        Run {
            label: label.to_owned(),
            steps: steps.len(),
            requests: r1 - r0,
            bytes: b1 - b0,
            seconds,
            whole: memory.whole_chunk_bytes() - w0,
        },
        values,
    )
}

fn open(
    dataset: Dataset,
    memory: &Arc<BlockCache>,
    spec: Variable,
) -> (OpenVariable, Arc<NetStats>, f64, u64) {
    let store = open_http(dataset.url(), TIMEOUT).unwrap();
    let net = store.net.clone().unwrap();
    let start = Instant::now();
    let variable = OpenVariable::open(&store, spec)
        .unwrap()
        .with_memory(Arc::clone(memory));
    let seconds = start.elapsed().as_secs_f64();
    let (requests, ..) = net.snapshot();
    (variable, net, seconds, requests)
}

fn steps_for(variable: &OpenVariable, from: &str, hours: i64, every: i64) -> Vec<u64> {
    let t0 = parse_utc(from).unwrap();
    (0..hours)
        .step_by(every as usize)
        .map(|h| {
            let (i, _, _) = variable
                .time()
                .bracket(t0 + h * 3600)
                .expect("inside the axis");
            i
        })
        .collect()
}

/// WeatherBench2 u10/v10, 24 hours at 50N 5W: cold, then from memory. The
/// M9 acceptance value doubles as a check, and one hour is checked against
/// the whole chunk decoded.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn wb2_wind_cost() {
    if !live() {
        return;
    }
    let memory = BlockCache::new(1 << 30);
    let (u, net_u, open_s, open_req) = open(Dataset::Wb2Era5Hourly, &memory, vars::WB2_U10);
    println!(
        "M14e | WB2 open u10: {open_s:.2} s, {open_req} requests, axis to {}",
        to_iso(u.time().last())
    );
    assert!(u.reads_blocks());
    let (v, net_v, ..) = open(Dataset::Wb2Era5Hourly, &memory, vars::WB2_V10);

    let day1 = steps_for(&u, "2020-07-27T00:00Z", 24, 1);
    let (run, values) = measure(
        "WB2 u10 24 h hourly, cold, 8 at a time",
        &u,
        &net_u,
        &memory,
        &day1,
        50.0,
        -5.0,
        8,
    );
    run.print();
    // 12Z is index 12 of the day; numcodecs: 9.382978439331055.
    let at_noon = values[12].expect("sea");
    assert!((at_noon - 9.382_978_439_331_055).abs() < 1e-5, "{at_noon}");
    assert!(run.bytes * 5 < run.whole, "block reads are far smaller");

    measure(
        "WB2 v10 24 h hourly, cold, 8 at a time",
        &v,
        &net_v,
        &memory,
        &day1,
        50.0,
        -5.0,
        8,
    )
    .0
    .print();
    let (warm, _) = measure(
        "WB2 u10 24 h hourly, from memory",
        &u,
        &net_u,
        &memory,
        &day1,
        50.2,
        -4.8,
        8,
    );
    warm.print();
    assert_eq!(warm.bytes, 0, "a second boat downloads nothing");
    println!(
        "M14e | memory after WB2: {:.1} MB",
        memory.size() as f64 / 1e6
    );
}

/// ARCO-ERA5 swh/mwd, 24 hours at 50N 5W, and u10 after WeatherBench2's end.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn arco_waves_cost() {
    if !live() {
        return;
    }
    let memory = BlockCache::new(1 << 30);
    let (swh, net, open_s, open_req) = open(Dataset::ArcoEra5, &memory, vars::ARCO_SWH);
    println!(
        "M14e | ARCO open swh: {open_s:.2} s, {open_req} requests, data to {}",
        to_iso(swh.time().last())
    );
    let (mwd, net_m, ..) = open(Dataset::ArcoEra5, &memory, vars::ARCO_MWD);
    let day = steps_for(&swh, "2020-07-27T00:00Z", 24, 1);
    let (run, values) = measure(
        "ARCO swh 24 h hourly, cold, 8 at a time",
        &swh,
        &net,
        &memory,
        &day,
        50.0,
        -5.0,
        8,
    );
    run.print();
    let hs = values[12].expect("sea");
    assert!((0.0..15.0).contains(&hs), "{hs}");
    let (run, values) = measure(
        "ARCO mwd 24 h hourly, cold, 8 at a time",
        &mwd,
        &net_m,
        &memory,
        &day,
        50.0,
        -5.0,
        8,
    );
    run.print();
    let dir_from = values[12].expect("sea");
    assert!((0.0..=360.0).contains(&dir_from), "{dir_from}");
    // Land is missing for waves: central France.
    let (_, land) = measure(
        "ARCO swh over land",
        &swh,
        &net,
        &memory,
        &day[..1],
        46.5,
        2.5,
        1,
    );
    assert_eq!(land[0], None);

    let (u, net_u, ..) = open(Dataset::ArcoEra5, &memory, vars::ARCO_U10);
    let fastnet = steps_for(&u, "2025-08-02T12:00Z", 24, 1);
    measure(
        "ARCO u10 24 h hourly (2025), cold, 8 at a time",
        &u,
        &net_u,
        &memory,
        &fastnet,
        50.0,
        -5.0,
        8,
    )
    .0
    .print();
}

/// CMEMS merged current at one point: utotal, utide, uo for 24 hours. One
/// geoChunk holds six months of the point in five blocks, and a day needs
/// one of them.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn cmems_current_cost() {
    if !live() {
        return;
    }
    let memory = BlockCache::new(1 << 30);
    for spec in [vars::CMEMS_UTOTAL, vars::CMEMS_UTIDE, vars::CMEMS_UO] {
        let (var, net, open_s, open_req) = open(Dataset::CmemsGlobalMerged, &memory, spec);
        println!(
            "M14e | CMEMS open {}: {open_s:.2} s, {open_req} requests",
            spec.array
        );
        let day = steps_for(&var, "2025-08-03T00:00Z", 24, 1);
        let (run, values) = measure(
            &format!("CMEMS {} 24 h at a point, cold", spec.array),
            &var,
            &net,
            &memory,
            &day,
            50.0,
            -5.0,
            8,
        );
        run.print();
        let v = values[0].expect("sea");
        assert!(v.abs() < 5.0, "{v}");
        if spec.array == "utotal" {
            assert!((v - -0.055_664_062_5).abs() < 1e-6, "{v}");
        }
    }
    // An all-land box (the Sahara) is not written: the archive answers 403,
    // which is "no data", not an error, and the value is missing.
    let (var, net, ..) = open(Dataset::CmemsGlobalMerged, &memory, vars::CMEMS_UTOTAL);
    let t = parse_utc("2025-08-03T00:00Z").unwrap();
    let (_, _, missing_before) = net.snapshot();
    assert_eq!(var.sample(t, 25.0, 15.0).unwrap(), None);
    let (_, _, missing_after) = net.snapshot();
    assert!(
        missing_after > missing_before,
        "the 403 was counted as missing"
    );
}

/// The provider over the real archives, one position per path through the
/// tiers: NW Shelf with ARCO-ERA5 wind (2025), NW Shelf with WeatherBench2
/// wind (2020), IBI off Portugal, the global merged current in the
/// mid-Atlantic after 2020-11, and GlobCurrent before it.
#[test]
#[ignore = "network; run with PE_TEST_LIVE=1"]
fn provider_end_to_end() {
    use pe_env::{Access, Interval, Options, Point, Provider, Reanalysis};
    if !live() {
        return;
    }
    let p =
        Reanalysis::new(Access::Http { timeout: TIMEOUT }, 8).with_memory(BlockCache::new(1 << 30));
    let at = |t: &str, lat: f64, lon: f64| Point {
        t: parse_utc(t).unwrap(),
        lat,
        lon,
    };
    let points = [
        at("2025-08-03T00:30Z", 49.86, -5.13),
        at("2020-07-27T12:00Z", 50.0, -5.0),
        at("2019-06-01T06:00Z", 36.0, -9.5),
        at("2023-06-01T06:00Z", 30.0, -40.0),
        at("2019-06-01T06:00Z", 30.0, -40.0),
    ];
    let options = Options {
        interval: Interval::Hourly,
        stokes_drift: false,
        parts: pe_env::Parts::ALL,
    };
    let start = Instant::now();
    let got = p
        .sample(&points, &options, &Arc::new(AtomicBool::new(false)))
        .unwrap();
    let (requests, bytes) = p.net_totals();
    println!(
        "provider: {:.1} s, {requests} requests, {:.2} MB (whole chunks: {:.2} MB)",
        start.elapsed().as_secs_f64(),
        bytes as f64 / 1e6,
        p.memory().whole_chunk_bytes() as f64 / 1e6
    );
    // The same provider again (the second boat of a session): archives
    // already open, blocks in memory.
    let again = Instant::now();
    let second = p
        .sample(&points, &options, &Arc::new(AtomicBool::new(false)))
        .unwrap();
    println!(
        "provider, second time: {:.2} s, {} more bytes",
        again.elapsed().as_secs_f64(),
        p.net_totals().1 - bytes
    );
    assert_eq!(second, got);
    for (point, env) in points.iter().zip(&got) {
        println!("{} {:?}", to_iso(point.t), env);
    }
    let wind = |i: usize| got[i].wind.unwrap().dataset;
    let current = |i: usize| got[i].current.map(|c| c.dataset);
    assert_eq!(wind(0), Dataset::ArcoEra5);
    assert_eq!(wind(1), Dataset::Wb2Era5Hourly);
    assert!((got[1].wind.unwrap().u - 9.382_978_439_331_055).abs() < 1e-5);
    assert_eq!(current(0), Some(Dataset::CmemsNwsMy));
    assert!((got[0].current.unwrap().u - 0.003_570_6).abs() < 1e-4);
    assert_eq!(current(1), Some(Dataset::CmemsNwsMy));
    assert_eq!(current(2), Some(Dataset::CmemsIbiMy));
    assert_eq!(current(3), Some(Dataset::CmemsGlobalMerged));
    assert_eq!(current(4), Some(Dataset::GlobCurrentMy));
    for env in &got {
        let waves = env.waves.unwrap();
        assert!((0.0..15.0).contains(&waves.hs.unwrap()));
    }
}
