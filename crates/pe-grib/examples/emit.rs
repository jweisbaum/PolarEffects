//! Writes a sample GRIB2 file, for checking against an external decoder
//! (ecCodes in CI, `tools/check-grib.sh`). Ported from VectorEffects'
//! `ve-grib` example, with regional grids.
//!
//!     cargo run -p pe-grib --example emit -- sample.grib2
//!
//! Two regional grids, each at two forecast hours: the Channel, across the
//! prime meridian (348.5°E to 0.75°E), and the Pacific, across the
//! antimeridian (168°E to 192°E). Every field is analytic and asymmetric:
//! `u` = latitude / 10, `v` = longitude / 10 with longitude in [−180, 180),
//! wave height = latitude / 10 with the northern half missing (a bitmap),
//! wave direction = longitude + 180, current `u` = forecast hour / 100 and
//! current `v` = −latitude / 100. A transposed axis, a grid in the wrong
//! place or a misread bitmap shows in the decoded values.

use pe_grib::writer::{GridSpec, MessageSpec, Parameter, ReferenceTime};
use pe_grib::{GribFile, NATIVE_STEP, packing, region};

/// The value each parameter holds at a point and hour.
pub fn field(parameter: Parameter, lon: f64, lat: f64, hour: u32, north_of: f64) -> f32 {
    let value = match parameter {
        Parameter::WindU => lat / 10.0,
        Parameter::WindV => lon / 10.0,
        Parameter::WaveHeight if lat > north_of => f64::NAN,
        Parameter::WaveHeight => lat / 10.0,
        Parameter::WaveDirection => lon + 180.0,
        Parameter::CurrentU => f64::from(hour) / 100.0,
        Parameter::CurrentV => -lat / 100.0,
    };
    value as f32
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "sample.grib2".to_owned());
    let grids: [GridSpec; 2] = [
        region(&[(50.8, -1.3), (49.9, -9.5)], 2.0, NATIVE_STEP).ok_or("no region")?,
        region(&[(20.0, 170.0), (21.0, -170.0)], 2.0, NATIVE_STEP).ok_or("no region")?,
    ];
    let parameters = [
        Parameter::WindU,
        Parameter::WindV,
        Parameter::WaveHeight,
        Parameter::WaveDirection,
        Parameter::CurrentU,
        Parameter::CurrentV,
    ];
    let mut file = GribFile::create(std::path::Path::new(&path))?;
    for grid in grids {
        let north_of = (grid.lat(0) + grid.lat(grid.nj - 1)) / 2.0;
        for hour in [0u32, 1] {
            for parameter in parameters {
                let values: Vec<f32> = grid
                    .points()
                    .map(|(lon, lat)| field(parameter, lon, lat, hour, north_of))
                    .collect();
                let spec = MessageSpec {
                    parameter,
                    grid,
                    // 2020-07-27T12:00Z.
                    reference_time: ReferenceTime::from_epoch(1_595_851_200)?,
                    forecast_hour: hour,
                    centre: 255,
                    bits: packing::BITS_PER_VALUE,
                };
                file.write(&spec, &values)?;
            }
        }
    }
    let written = file.commit()?;
    println!(
        "wrote {path}: {} bytes, {} messages",
        written.bytes, written.messages
    );
    Ok(())
}
