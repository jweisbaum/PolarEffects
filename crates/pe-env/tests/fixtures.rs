#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! Reading recorded archive chunks end to end: store → blosc → grid →
//! stencil → value, against values decoded independently.
//!
//! # How the fixtures were recorded (M3, 2026-09-28)
//!
//! - `wb2-crop/`: the WeatherBench2 hourly store's metadata, `time/0` and
//!   `time/7` as served, and the `10m_{u,v}_component_of_wind/539724.0.0`
//!   chunks (2020-07-27T12Z) decoded with numcodecs 0.12 (c-blosc), cropped to
//!   55–45N × 350–359.75E (rows 140–180, columns 1400–1439 of the global
//!   grid) and re-encoded with numcodecs `Blosc(lz4, clevel 5, shuffle)`; the
//!   array and axis `.zarray` shapes were cut to match. A global chunk is
//!   3.3 MB, too big to commit.
//! - `cmems-merged-geo/`: the Copernicus Marine merged-current geoChunked
//!   store's metadata, `time/0`, `time/2`, the axes and the chunk
//!   `utotal/9.0.97.262` exactly as served (879 KB): 178 days from
//!   2025-03-22 of the 1/12° box 49.33–50.58N × 5.33–4.75W, which includes
//!   the Lizard, so it has land (fill) cells.
//!
//! The reference values below were read from the same bytes with numcodecs,
//! an implementation independent of `blosc.rs`.

use std::path::PathBuf;

use pe_env::dataset::{OpenVariable, vars};
use pe_env::store::open_dir;
use pe_env::time::parse_utc;

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

/// The M9 acceptance point, early: u10 and v10 at 2020-07-27T12Z, 50N 5W.
/// numcodecs gives u = 9.382978439331055, v = 5.924107074737549 at row 160,
/// column 1420 of the global chunk.
#[test]
fn wb2_wind_at_a_grid_node_matches_numcodecs() {
    let store = open_dir(&fixture("wb2-crop")).unwrap().store;
    let t = parse_utc("2020-07-27T12:00Z").unwrap();
    let u = OpenVariable::open(&store, vars::WB2_U10).unwrap();
    let v = OpenVariable::open(&store, vars::WB2_V10).unwrap();
    assert_eq!(u.time().first, parse_utc("1959-01-01").unwrap());
    assert_eq!(u.time().step, 3600);
    assert_eq!(u.time().len, 561_264);
    assert_eq!(u.grid().lat.first, 55.0);
    assert_eq!(u.grid().lat.step, -0.25);
    let got_u = u.sample(t, 50.0, -5.0).unwrap().expect("inside");
    let got_v = v.sample(t, 50.0, 355.0).unwrap().expect("inside");
    assert!(close(got_u, 9.382_978_439_331_055, 1e-6), "{got_u}");
    assert!(close(got_v, 5.924_107_074_737_549, 1e-6), "{got_v}");
}

/// Between nodes: 50.1N 4.9W sits 0.6 of the way from row 159 (50.25N) to
/// row 160 (50.0N) and 0.4 from column 1420 (355.0E) to 1421 (355.25E).
/// Weights 0.24, 0.16, 0.36, 0.24 on the numcodecs corner values.
#[test]
fn wb2_wind_between_nodes_is_bilinear() {
    let store = open_dir(&fixture("wb2-crop")).unwrap().store;
    let u = OpenVariable::open(&store, vars::WB2_U10).unwrap();
    let t = parse_utc("2020-07-27T12:00Z").unwrap();
    let corners = [
        8.838_661_193_847_656,
        8.494_581_222_534_18,
        9.382_978_439_331_055,
        9.491_680_145_263_672,
    ];
    let expected = 0.24 * corners[0] + 0.16 * corners[1] + 0.36 * corners[2] + 0.24 * corners[3];
    let got = u.sample(t, 50.1, -4.9).unwrap().expect("inside");
    assert!(close(got, expected, 1e-5), "{got} vs {expected}");
}

/// Only the 12Z chunk was recorded, so an hour later reads the store's
/// NaN fill for a missing chunk and is missing, not zero; and outside the
/// cropped grid is outside.
#[test]
fn wb2_a_missing_chunk_and_outside_the_grid_are_missing() {
    let store = open_dir(&fixture("wb2-crop")).unwrap().store;
    let u = OpenVariable::open(&store, vars::WB2_U10).unwrap();
    let t = parse_utc("2020-07-27T14:00Z").unwrap();
    assert_eq!(u.sample(t, 50.0, -5.0).unwrap(), None);
    let t = parse_utc("2020-07-27T12:00Z").unwrap();
    assert_eq!(u.sample(t, 30.0, -5.0).unwrap(), None);
    // Half an hour on: one side missing and the other at weight 0.5 still
    // gives the recorded side.
    let half = parse_utc("2020-07-27T12:30Z").unwrap();
    let got = u.sample(half, 50.0, -5.0).unwrap().expect("nearer side");
    assert!(close(got, 9.382_978_439_331_055, 1e-6));
}

/// CMEMS `utotal` at 2025-08-03T00Z (index 41664, chunk row 3216), 50N 5W
/// (chunk row 8, column 4): numcodecs gives -0.0556640625, and the next
/// hour -0.1474609375; half past is their mean.
#[test]
fn cmems_current_at_a_node_and_between_hours() {
    let store = open_dir(&fixture("cmems-merged-geo")).unwrap().store;
    let u = OpenVariable::open(&store, vars::CMEMS_UTOTAL).unwrap();
    assert_eq!(u.time().first, parse_utc("2020-11-01").unwrap());
    assert_eq!(u.chunk_shape().unwrap(), vec![4272, 1, 16, 8]);
    let t = parse_utc("2025-08-03T00:00Z").unwrap();
    let got = u.sample(t, 50.0, -5.0).unwrap().expect("sea");
    assert!(close(got, -0.055_664_062_5, 1e-6), "{got}");
    let got = u.sample(t + 1800, 50.0, -5.0).unwrap().expect("sea");
    assert!(
        close(got, (-0.055_664_062_5 - 0.147_460_937_5) / 2.0, 1e-6),
        "{got}"
    );
}

/// The land rule (spec.md 7.5): at 50.05N 5.2W the two northern corners
/// are fill (the Lizard), so the southern two are renormalised:
/// (0.16 × -0.021484375 + 0.24 × -0.04296875) / 0.4 = -0.034375. With all
/// four corners on land the value is missing.
#[test]
fn cmems_land_corners_are_left_out_and_all_land_is_missing() {
    let store = open_dir(&fixture("cmems-merged-geo")).unwrap().store;
    let u = OpenVariable::open(&store, vars::CMEMS_UTOTAL).unwrap();
    let t = parse_utc("2025-08-03T00:00Z").unwrap();
    let got = u.sample(t, 50.05, -5.2).unwrap().expect("two sea corners");
    assert!(close(got, -0.034_375, 1e-5), "{got}");
    let lat = -80.0 + (1552.0 + 9.5) / 12.0;
    let lon = -180.0 + (2096.0 + 1.5) / 12.0;
    assert_eq!(u.sample(t, lat, lon).unwrap(), None);
}

/// A chunk that is not in the store (the archive answers 403) is fill, and
/// fill is missing, never 9.97e36 m/s.
#[test]
fn cmems_an_absent_chunk_is_missing() {
    let store = open_dir(&fixture("cmems-merged-geo")).unwrap().store;
    let u = OpenVariable::open(&store, vars::CMEMS_UTOTAL).unwrap();
    let t = parse_utc("2025-08-03T00:00Z").unwrap();
    assert_eq!(u.sample(t, 40.0, -20.0).unwrap(), None);
}

/// A variable the store does not have is an open error, not a panic.
#[test]
fn an_absent_variable_is_refused() {
    let store = open_dir(&fixture("cmems-merged-geo")).unwrap().store;
    assert!(OpenVariable::open(&store, vars::CMEMS_VTIDE).is_err());
}
