#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "test code; clippy's allow-in-tests does not reach tests/"
)]
//! The writer against the in-repo reader (a separate parse of the bytes):
//! regional grids across both seams, every parameter, a bitmap, the time
//! convention, and values back within the 16-bit packing step. ecCodes
//! checks the same shapes in CI (`tools/check-grib.sh`).

use pe_grib::reader::decode_all;
use pe_grib::writer::{GridSpec, MessageSpec, Parameter, ReferenceTime};
use pe_grib::{GribFile, NATIVE_STEP, packing, region};

/// Values within half a packing step of the field's own range.
fn tolerance(values: &[f32]) -> f32 {
    let finite = values.iter().copied().filter(|v| v.is_finite());
    let (lo, hi) = finite.fold((f32::INFINITY, f32::NEG_INFINITY), |(a, b), v| {
        (a.min(v), b.max(v))
    });
    ((hi - lo) / 65535.0).max(1e-6) * 1.5
}

fn write_and_read(
    grid: GridSpec,
    fields: &[(Parameter, Vec<f32>)],
    hours: &[u32],
) -> Vec<pe_grib::reader::Decoded> {
    let dir = std::env::temp_dir().join(format!(
        "pe-grib-roundtrip-{}-{}",
        grid.lo1,
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("out.grib2");
    let mut file = GribFile::create(&path).unwrap();
    for &hour in hours {
        for (parameter, values) in fields {
            let spec = MessageSpec {
                parameter: *parameter,
                grid,
                reference_time: ReferenceTime::from_epoch(1_595_851_200).unwrap(),
                forecast_hour: hour,
                centre: 255,
                bits: packing::BITS_PER_VALUE,
            };
            file.write(&spec, values).unwrap();
        }
    }
    file.commit().unwrap();
    let bytes = std::fs::read(&path).unwrap();
    let _ = std::fs::remove_dir_all(dir);
    decode_all(&bytes)
}

#[test]
fn a_channel_grid_across_the_prime_meridian_round_trips() {
    let grid = region(&[(50.8, -1.3), (49.9, -9.5)], 2.0, NATIVE_STEP).unwrap();
    let points: Vec<(f64, f64)> = grid.points().collect();
    let u: Vec<f32> = points.iter().map(|(_, lat)| (lat / 10.0) as f32).collect();
    let v: Vec<f32> = points.iter().map(|(lon, _)| (lon / 10.0) as f32).collect();
    // Land in the north-east: missing.
    let hs: Vec<f32> = points
        .iter()
        .map(|(lon, lat)| {
            if *lat > 51.0 && *lon > -3.0 {
                f32::NAN
            } else {
                (*lat - 45.0) as f32
            }
        })
        .collect();
    let decoded = write_and_read(
        grid,
        &[
            (Parameter::WindU, u.clone()),
            (Parameter::WindV, v.clone()),
            (Parameter::WaveHeight, hs.clone()),
        ],
        &[0, 1, 2],
    );
    assert_eq!(decoded.len(), 9);
    for (k, m) in decoded.iter().enumerate() {
        assert_eq!(
            m.forecast_hour,
            (k / 3) as u32,
            "hours are offsets from the first"
        );
        assert_eq!(
            m.reference_time,
            ReferenceTime::from_epoch(1_595_851_200).unwrap()
        );
        assert_eq!((m.ni, m.nj), (50, 22));
        assert_eq!(
            (m.la1, m.lo1, m.la2, m.lo2),
            (53_000_000, 348_500_000, 47_750_000, 750_000)
        );
        assert_eq!(
            (m.di, m.dj, m.scanning_mode, m.resolution_flags),
            (250_000, 250_000, 0, 0x30)
        );
        assert_eq!(m.bits, 16);
        let want = [&u, &v, &hs][k % 3];
        let tol = tolerance(want);
        for (i, (a, b)) in want.iter().zip(&m.values).enumerate() {
            if a.is_nan() {
                assert!(b.is_nan(), "message {k} node {i}: {b} should be missing");
            } else {
                assert!((a - b).abs() <= tol, "message {k} node {i}: {b} vs {a}");
            }
        }
    }
    assert_eq!(
        (
            decoded[0].discipline,
            decoded[0].category,
            decoded[0].number
        ),
        (0, 2, 2)
    );
    assert_eq!(
        (decoded[0].surface_type, decoded[0].surface_value),
        (103, 10)
    );
    assert_eq!(
        (
            decoded[2].discipline,
            decoded[2].category,
            decoded[2].number
        ),
        (10, 0, 3)
    );
    assert_eq!(decoded[2].surface_type, 1);
}

#[test]
fn a_pacific_grid_across_the_antimeridian_round_trips() {
    let grid = region(&[(20.0, 170.0), (21.0, -170.0)], 2.0, NATIVE_STEP).unwrap();
    let points: Vec<(f64, f64)> = grid.points().collect();
    let dir: Vec<f32> = points.iter().map(|(lon, _)| (lon + 180.0) as f32).collect();
    let cu: Vec<f32> = points
        .iter()
        .map(|(_, lat)| (-lat / 100.0) as f32)
        .collect();
    let decoded = write_and_read(
        grid,
        &[
            (Parameter::WaveDirection, dir.clone()),
            (Parameter::CurrentU, cu.clone()),
            (Parameter::CurrentV, vec![0.25; points.len()]),
        ],
        &[0],
    );
    let m = &decoded[0];
    assert_eq!((m.lo1, m.lo2, m.ni), (168_000_000, 192_000_000, 97));
    // Column 48 is 180°: the app's −180, direction 0; column 47 is 179.75E.
    assert!((m.values[48] - 0.0).abs() < 0.01, "{}", m.values[48]);
    assert!((m.values[47] - 359.75).abs() < 0.01, "{}", m.values[47]);
    assert_eq!((m.discipline, m.category, m.number), (10, 0, 14));
    assert_eq!(
        (
            decoded[1].category,
            decoded[1].number,
            decoded[1].surface_type
        ),
        (1, 2, 160)
    );
    let tol = tolerance(&cu);
    for (a, b) in cu.iter().zip(&decoded[1].values) {
        assert!((a - b).abs() <= tol);
    }
    // A constant field packs to nothing and still reads back.
    assert_eq!(decoded[2].bits, 0);
    assert!(decoded[2].values.iter().all(|v| *v == 0.25));
}

/// An hour the archive does not have is a message of nothing but missing
/// values, not zeros.
#[test]
fn an_all_missing_field_reads_back_missing() {
    let grid = region(&[(0.0, 0.0)], 1.0, NATIVE_STEP).unwrap();
    let n = grid.point_count() as usize;
    let decoded = write_and_read(grid, &[(Parameter::WindU, vec![f32::NAN; n])], &[5]);
    assert_eq!(decoded[0].values.len(), n);
    assert!(decoded[0].values.iter().all(|v| v.is_nan()));
    assert_eq!(decoded[0].forecast_hour, 5);
}
