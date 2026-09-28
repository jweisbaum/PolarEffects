#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The provider end to end: positions in, wind, waves and current out,
//! against values decoded independently of this crate.
//!
//! # References
//!
//! - **The M9 acceptance value**: u10 at 2020-07-27T12Z, 50N 5W from the
//!   `wb2-crop` fixture (the WeatherBench2 chunk `539724.0.0` as served,
//!   cropped; see `tests/fixtures.rs` for how it was recorded). numcodecs
//!   0.12.1 (c-blosc, independent of `blosc.rs`) decodes row 160, column
//!   1420 of the global chunk as u = 9.382978439331055, v =
//!   5.924107074737549.
//! - **Currents**: `currents-crop/` holds, for each of the NW Shelf, global
//!   merged and GlobCurrent geoChunked stores, the one chunk around
//!   49.86N 5.13W for 48 hours from 2025-08-02T00Z, decoded with numcodecs,
//!   cut to those hours and re-encoded with the same blosc settings
//!   (`currents-crop/record.py`). The expected values below were computed
//!   from the numcodecs-decoded corner values with the script's own
//!   bilinear weights in Python, at 2025-08-03T00:30Z (halfway between the
//!   00Z and 01Z steps).
//! - **ERA5 waves and the WeatherBench2 → ARCO switch** use small synthetic
//!   stores written here, uncompressed, with hand-chosen values.

use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;

use pe_env::dataset::Dataset;
use pe_env::time::parse_utc;
use pe_env::{Access, EnvPoint, Interval, Options, Point, Provider, Reanalysis};

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(name)
}

fn close(a: f64, b: f64, tol: f64) -> bool {
    (a - b).abs() <= tol
}

fn provider(dirs: &[(Dataset, PathBuf)]) -> Reanalysis {
    Reanalysis::new(Access::Dirs(dirs.iter().cloned().collect()), 4)
}

fn sample(p: &Reanalysis, points: &[Point], options: Options) -> Vec<EnvPoint> {
    p.sample(points, &options, &AtomicBool::new(false)).unwrap()
}

const HOURLY: Options = Options {
    interval: Interval::Hourly,
    stokes_drift: false,
};

/// The M9 acceptance test: sampled u10 at 2020-07-27 12Z, 50N 5W matches
/// the value numcodecs decodes from the same chunk.
#[test]
fn acceptance_u10_at_50n_5w_matches_numcodecs() {
    let p = provider(&[(Dataset::Wb2Era5Hourly, fixture("wb2-crop"))]);
    let t = parse_utc("2020-07-27T12:00Z").unwrap();
    let got = sample(
        &p,
        &[Point {
            t,
            lat: 50.0,
            lon: -5.0,
        }],
        HOURLY,
    );
    let wind = got[0].wind.expect("wind");
    assert_eq!(wind.dataset, Dataset::Wb2Era5Hourly);
    assert!(close(wind.u, 9.382_978_439_331_055, 1e-6), "{}", wind.u);
    assert!(close(wind.v, 5.924_107_074_737_549, 1e-6), "{}", wind.v);
    // No wave or current dataset in this provider: nothing invented.
    assert_eq!(got[0].waves, None);
    assert_eq!(got[0].current, None);
}

/// A position's answer does not depend on its batch: alone or among
/// others, the same value.
#[test]
fn a_position_answers_the_same_in_any_batch() {
    let p = provider(&[(Dataset::Wb2Era5Hourly, fixture("wb2-crop"))]);
    let t = parse_utc("2020-07-27T12:00Z").unwrap();
    let a = Point {
        t,
        lat: 50.1,
        lon: -4.9,
    };
    let b = Point {
        t,
        lat: 49.0,
        lon: -8.0,
    };
    let alone = sample(&p, &[a], HOURLY);
    let together = sample(&p, &[b, a, b], HOURLY);
    assert_eq!(alone[0], together[1]);
}

const CURRENT_T: &str = "2025-08-03T00:30Z";
const CURRENT_AT: (f64, f64) = (49.86, -5.13);

fn current_point() -> Point {
    Point {
        t: parse_utc(CURRENT_T).unwrap(),
        lat: CURRENT_AT.0,
        lon: CURRENT_AT.1,
    }
}

fn currents(which: &[&str]) -> Reanalysis {
    let dirs: Vec<(Dataset, PathBuf)> = which
        .iter()
        .map(|name| {
            let dataset = match *name {
                "nws" => Dataset::CmemsNwsMy,
                "merged" => Dataset::CmemsGlobalMerged,
                "gc-my" => Dataset::GlobCurrentMy,
                other => panic!("{other}"),
            };
            (dataset, fixture("currents-crop").join(name))
        })
        .collect();
    provider(&dirs)
}

/// Tier 1 first: the NW Shelf reanalysis, int16 unpacked with its
/// `scale_factor` (0.0010000000474974513). Python: u 0.0035706098189213498,
/// v -0.09680439592297212.
#[test]
fn the_regional_tier_comes_first_and_is_unpacked() {
    let got = sample(
        &currents(&["nws", "merged", "gc-my"]),
        &[current_point()],
        HOURLY,
    );
    let c = got[0].current.expect("a current");
    assert_eq!(c.dataset, Dataset::CmemsNwsMy);
    assert!(close(c.u, 0.003_570_609_818_921_35, 2e-5), "{}", c.u);
    assert!(close(c.v, -0.096_804_395_922_972_12, 2e-5), "{}", c.v);
}

/// Without the regional tier, the global merged current is uo + utide.
/// Python: u -0.11844464639369347, v -0.125221342946075; with Stokes
/// drift, u -0.07664782503623667, v -0.1318619143035318.
#[test]
fn the_merged_tier_adds_the_tide_and_stokes_only_when_asked() {
    let p = currents(&["merged", "gc-my"]);
    let c = sample(&p, &[current_point()], HOURLY)[0]
        .current
        .expect("a current");
    assert_eq!(c.dataset, Dataset::CmemsGlobalMerged);
    assert!(close(c.u, -0.118_444_646_393_693_47, 2e-5), "{}", c.u);
    assert!(close(c.v, -0.125_221_342_946_075, 2e-5), "{}", c.v);
    let stokes = Options {
        stokes_drift: true,
        ..HOURLY
    };
    let c = sample(&p, &[current_point()], stokes)[0]
        .current
        .expect("a current");
    assert!(close(c.u, -0.076_647_825_036_236_67, 2e-5), "{}", c.u);
    assert!(close(c.v, -0.131_861_914_303_531_8, 2e-5), "{}", c.v);
}

/// GlobCurrent last, at its surface level (index 1 of elevation [-15, 0]),
/// int16 with scale 0.001. Python: u 0.14524359999999997, v
/// -0.04297100000000005.
#[test]
fn globcurrent_is_the_last_tier_at_the_surface() {
    let c = sample(&currents(&["gc-my"]), &[current_point()], HOURLY)[0]
        .current
        .expect("a current");
    assert_eq!(c.dataset, Dataset::GlobCurrentMy);
    assert!(close(c.u, 0.145_243_6, 2e-5), "{}", c.u);
    assert!(close(c.v, -0.042_971, 2e-5), "{}", c.v);
}

/// A position no tier has data for has no current (the recorded stores
/// hold one chunk each; elsewhere is a missing chunk, i.e. no data), and
/// one after every store ends has none either.
#[test]
fn no_tier_with_data_means_no_current() {
    let p = currents(&["nws", "merged", "gc-my"]);
    let elsewhere = Point {
        lat: 45.0,
        lon: -10.0,
        ..current_point()
    };
    let later = Point {
        t: parse_utc("2025-09-01T00:00Z").unwrap(),
        ..current_point()
    };
    let got = sample(&p, &[elsewhere, later], HOURLY);
    assert_eq!(got[0].current, None);
    assert_eq!(got[1].current, None);
}

// ---------------------------------------------------------- synthetic ERA5

/// An array's value at each grid point, per time step.
type Field<'a> = dyn Fn(usize) -> [f32; 9] + 'a;

/// Writes an uncompressed Zarr v2 store: a 3 × 3 grid at 49–51N, 355–357E
/// (0–360 like ERA5), hourly from `first_hour` (hours since `epoch`) for
/// `hours` steps, with `arrays` giving each array's value at each step.
fn write_era5(
    dir: &Path,
    epoch: &str,
    first_hour: i64,
    hours: usize,
    arrays: &[(&str, &Field<'_>)],
) {
    let _ = std::fs::remove_dir_all(dir);
    std::fs::create_dir_all(dir).unwrap();
    std::fs::write(dir.join(".zgroup"), r#"{"zarr_format": 2}"#).unwrap();
    let array = |name: &str,
                 shape: &[usize],
                 chunks: &[usize],
                 dtype: &str,
                 attrs: serde_json::Value| {
        let d = dir.join(name);
        std::fs::create_dir_all(&d).unwrap();
        let meta = serde_json::json!({
            "zarr_format": 2, "shape": shape, "chunks": chunks, "dtype": dtype,
            "compressor": null, "filters": null, "order": "C",
            "fill_value": if dtype == "<f4" { serde_json::json!("NaN") } else { serde_json::Value::Null }
        });
        std::fs::write(d.join(".zarray"), meta.to_string()).unwrap();
        std::fs::write(d.join(".zattrs"), attrs.to_string()).unwrap();
        d
    };
    let f4 = |values: &[f32]| {
        values
            .iter()
            .flat_map(|v| v.to_le_bytes())
            .collect::<Vec<u8>>()
    };
    let lat = array(
        "latitude",
        &[3],
        &[3],
        "<f4",
        serde_json::json!({"_ARRAY_DIMENSIONS": ["latitude"]}),
    );
    std::fs::write(lat.join("0"), f4(&[51.0, 50.0, 49.0])).unwrap();
    let lon = array(
        "longitude",
        &[3],
        &[3],
        "<f4",
        serde_json::json!({"_ARRAY_DIMENSIONS": ["longitude"]}),
    );
    std::fs::write(lon.join("0"), f4(&[355.0, 356.0, 357.0])).unwrap();
    let time = array(
        "time",
        &[hours],
        &[hours],
        "<i8",
        serde_json::json!({"_ARRAY_DIMENSIONS": ["time"], "units": format!("hours since {epoch}")}),
    );
    let steps: Vec<u8> = (0..hours as i64)
        .flat_map(|k| (first_hour + k).to_le_bytes())
        .collect();
    std::fs::write(time.join("0"), steps).unwrap();
    for (name, values) in arrays {
        let d = array(
            name,
            &[hours, 3, 3],
            &[1, 3, 3],
            "<f4",
            serde_json::json!({"_ARRAY_DIMENSIONS": ["time", "latitude", "longitude"]}),
        );
        for step in 0..hours {
            std::fs::write(d.join(format!("{step}.0.0")), f4(&values(step))).unwrap();
        }
    }
}

fn temp(label: &str) -> PathBuf {
    std::env::temp_dir().join(format!("pe-sampler-{label}-{}", std::process::id()))
}

/// Hours since 1959-01-01 and 1900-01-01 of 2020-07-27T00Z.
fn hours_since(epoch: &str) -> i64 {
    (parse_utc("2020-07-27T00:00Z").unwrap() - parse_utc(epoch).unwrap()) / 3600
}

/// Two synthetic stores: WeatherBench2 with 6 hours from 2020-07-27T00Z
/// where u = step, v = 0; ARCO with 12 hours where u = 100 + step², v = 1,
/// wave height = step / 10 except a land (NaN) corner, and wave direction
/// 350° on the west column and 10° elsewhere.
fn era5_pair(label: &str) -> (Reanalysis, PathBuf) {
    let root = temp(label);
    let flat = |x: f32| [x; 9];
    write_era5(
        &root.join("wb2"),
        "1959-01-01",
        hours_since("1959-01-01"),
        6,
        &[
            ("10m_u_component_of_wind", &|s| flat(s as f32)),
            ("10m_v_component_of_wind", &|_| flat(0.0)),
        ],
    );
    write_era5(
        &root.join("arco"),
        "1900-01-01",
        hours_since("1900-01-01"),
        12,
        &[
            ("10m_u_component_of_wind", &|s| flat(100.0 + (s * s) as f32)),
            ("10m_v_component_of_wind", &|_| flat(1.0)),
            (
                "significant_height_of_combined_wind_waves_and_swell",
                &|s| {
                    let mut v = flat(s as f32 / 10.0);
                    v[0] = f32::NAN;
                    v
                },
            ),
            ("mean_wave_direction", &|_| {
                [350.0, 10.0, 10.0, 350.0, 10.0, 10.0, 350.0, 10.0, 10.0]
            }),
        ],
    );
    let p = provider(&[
        (Dataset::Wb2Era5Hourly, root.join("wb2")),
        (Dataset::ArcoEra5, root.join("arco")),
    ]);
    (p, root)
}

fn at(hour: f64, lat: f64, lon: f64) -> Point {
    Point {
        t: parse_utc("2020-07-27T00:00Z").unwrap() + (hour * 3600.0) as i64,
        lat,
        lon,
    }
}

/// D12: WeatherBench2 while both bracketing hours are in it, ARCO-ERA5
/// after. 02:30 is WB2 (u 2.5); 05:30 brackets 05 and 06, and WB2 has no
/// 06, so ARCO (u 100 + (25 + 36)/2 = 130.5); 08:00 is ARCO (u 164).
#[test]
fn wind_switches_from_weatherbench2_to_arco_where_it_ends() {
    let (p, root) = era5_pair("switch");
    let got = sample(
        &p,
        &[
            at(2.5, 50.0, -4.0),
            at(5.5, 50.0, -4.0),
            at(8.0, 50.0, -4.0),
        ],
        HOURLY,
    );
    let w: Vec<_> = got.iter().map(|g| g.wind.expect("wind")).collect();
    assert_eq!(w[0].dataset, Dataset::Wb2Era5Hourly);
    assert!(close(w[0].u, 2.5, 1e-9));
    assert_eq!(w[1].dataset, Dataset::ArcoEra5);
    assert!(close(w[1].u, 130.5, 1e-9), "{}", w[1].u);
    assert_eq!(w[2].dataset, Dataset::ArcoEra5);
    assert!(close(w[2].u, 164.0, 1e-9));
    let _ = std::fs::remove_dir_all(root);
}

/// D19: 3-hourly reads 06Z and 09Z around 07:00 and interpolates between
/// them: u = 100 + 36 + (81 − 36) / 3 = 151, where hourly gives 100 + 49.
#[test]
fn three_hourly_interpolates_between_three_hour_steps() {
    let (p, root) = era5_pair("coarse");
    let point = [at(7.0, 50.0, -4.0)];
    let hourly = sample(&p, &point, HOURLY)[0].wind.unwrap();
    let coarse = sample(
        &p,
        &point,
        Options {
            interval: Interval::ThreeHourly,
            ..HOURLY
        },
    )[0]
    .wind
    .unwrap();
    assert!(close(hourly.u, 149.0, 1e-9));
    assert!(close(coarse.u, 151.0, 1e-9), "{}", coarse.u);
    let _ = std::fs::remove_dir_all(root);
}

/// Waves: height bilinear with the land corner left out; direction as a
/// unit vector, so halfway between 350° and 10° is 0°, not 180°.
#[test]
fn wave_direction_interpolates_as_a_unit_vector_and_land_is_left_out() {
    let (p, root) = era5_pair("waves");
    // 50.5N 355.5E: the corners are rows 0–1 (51, 50N), columns 0–1 (355,
    // 356E), each at weight 1/4; the NW corner (51N 355E) is land.
    let got = sample(&p, &[at(4.0, 50.5, -4.5)], HOURLY);
    let waves = got[0].waves.expect("waves");
    assert_eq!(waves.dataset, Dataset::ArcoEra5);
    assert!(close(waves.hs.unwrap(), 0.4, 1e-6), "{:?}", waves.hs);
    let from = waves.from.unwrap();
    assert!(!(1e-6..=360.0 - 1e-6).contains(&from), "{from}");
    // On the land node itself with no weight elsewhere: no height.
    let land = sample(&p, &[at(4.0, 51.0, -5.0)], HOURLY);
    assert_eq!(land[0].waves.and_then(|w| w.hs), None);
    let _ = std::fs::remove_dir_all(root);
}

/// Before any archive's first hour there is nothing, not a zero.
#[test]
fn before_the_archives_there_is_nothing() {
    let (p, root) = era5_pair("before");
    let got = sample(&p, &[at(-5.0, 50.0, -4.0)], HOURLY);
    assert_eq!(got[0], EnvPoint::default());
    let _ = std::fs::remove_dir_all(root);
}

/// A set cancel flag stops the batch with Cancelled rather than a partial
/// answer.
#[test]
fn a_cancelled_batch_is_an_error_not_a_partial_answer() {
    let (p, root) = era5_pair("cancel");
    let err = p
        .sample(&[at(2.0, 50.0, -4.0)], &HOURLY, &AtomicBool::new(true))
        .expect_err("cancelled");
    assert!(matches!(err, pe_env::EnvError::Cancelled), "{err}");
    let _ = std::fs::remove_dir_all(root);
}
