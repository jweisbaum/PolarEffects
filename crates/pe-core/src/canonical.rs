//! Canonical numeric precision for the document.
//!
//! Copied from VectorEffects' `ve-core::canonical`, with the quantities
//! PolarExplorer stores.
//!
//! # Why this exists
//!
//! `serde_json`'s float *parser* is not correctly rounded unless its
//! `float_roundtrip` feature is on: it lands one ULP away from the true
//! nearest `f64` for roughly 10% of values. The writer is fine, so a raw `f64`
//! written to a project file does not always read back as the same value.
//! Numerically the error is irrelevant; what it breaks is *identity*: a
//! project would not equal itself across a save and a load, which defeats
//! byte-reproducible files (invariant 5) and makes "did this file change?"
//! unanswerable.
//!
//! The fix is to quantise at the serialisation boundary, in both directions.
//! A value with few enough significant digits is named exactly by its own
//! text under any correctly rounded parser, and the precisions below are far
//! finer than anything a polar, a GPS fix or a reanalysis grid can express.
//!
//! # Non-finite values
//!
//! JSON has no NaN or infinity, and `serde_json` would silently write `null`,
//! which then fails to read back: the project would save and never open again.
//! So the helpers here **refuse** to serialise a non-finite value, and a save
//! of a document holding one fails before anything is written.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Decimal places kept for angles and coordinates in degrees. About 0.1 mm
/// of latitude.
pub const DEGREE_PLACES: i32 = 9;
/// Decimal places kept for speeds in knots. A micro-knot.
pub const KNOT_PLACES: i32 = 6;
/// Decimal places kept for lengths in metres. A millimetre.
pub const METRE_PLACES: i32 = 3;
/// Decimal places kept for dimensionless quantities such as weights.
pub const RATIO_PLACES: i32 = 6;

/// Decimal places kept for the environment found at a track sample
/// (spec.md 7.5): wind and current speed to 0.01 kn, directions to 0.1°,
/// wave height to a centimetre. ERA5 and the current reanalyses resolve
/// nothing near that fine, and at this precision a track's environment is a
/// few kilobytes per thousand samples in the project file (D27).
pub const ENV_KNOT_PLACES: i32 = 2;
/// See [`ENV_KNOT_PLACES`].
pub const ENV_DEGREE_PLACES: i32 = 1;
/// See [`ENV_KNOT_PLACES`].
pub const ENV_METRE_PLACES: i32 = 2;

/// Rounds a speed found by the environment fetch (wind, current).
pub fn env_knots(value: f64) -> f64 {
    round(value, ENV_KNOT_PLACES)
}

/// Rounds a direction found by the environment fetch into [0, 360): 359.96
/// is 0.0, not 360.0.
pub fn env_degrees(value: f64) -> f64 {
    let d = round(value.rem_euclid(360.0), ENV_DEGREE_PLACES);
    if d >= 360.0 { 0.0 } else { d }
}

/// Rounds a wave height found by the environment fetch.
pub fn env_metres(value: f64) -> f64 {
    round(value, ENV_METRE_PLACES)
}

/// Rounds to `places` decimal places.
///
/// Values that are not finite, or large enough that scaling would overflow,
/// are returned unchanged: they cannot be scaled, and a non-finite one is
/// refused by the serde helpers below anyway.
pub fn round(value: f64, places: i32) -> f64 {
    if !value.is_finite() || value.abs() >= 1e15 {
        return value;
    }
    let factor = 10f64.powi(places);
    let rounded = (value * factor).round() / factor;
    // Normalise negative zero: `-0.0` and `0.0` would otherwise write as
    // different text for equal values.
    if rounded == 0.0 { 0.0 } else { rounded }
}

/// Rounds an angle or coordinate in degrees to canonical precision.
pub fn degrees(value: f64) -> f64 {
    round(value, DEGREE_PLACES)
}

/// Rounds a speed in knots to canonical precision.
pub fn knots(value: f64) -> f64 {
    round(value, KNOT_PLACES)
}

/// Rounds a length in metres to canonical precision.
pub fn metres(value: f64) -> f64 {
    round(value, METRE_PLACES)
}

/// Rounds a dimensionless value to canonical precision.
pub fn ratio(value: f64) -> f64 {
    round(value, RATIO_PLACES)
}

fn finite<E: serde::ser::Error>(value: f64) -> Result<f64, E> {
    if value.is_finite() {
        Ok(value)
    } else {
        Err(E::custom(format!(
            "{value} cannot be stored in a project file; only finite numbers can"
        )))
    }
}

/// Builds the four serde helper modules for one quantity: a plain value, an
/// optional one, a list, and a list of optional values (a polar grid row with
/// empty cells).
///
/// `#[serde(with)]` applies to the whole field type, not to what is inside
/// it, so each shape needs its own helper.
macro_rules! rounding_serde {
    ($plain:ident, $optional:ident, $list:ident, $optional_list:ident, $round:path, $what:literal) => {
        #[doc = concat!("Serde helper for ", $what, ".")]
        pub mod $plain {
            use super::*;

            /// Rounds, then serialises.
            pub fn serialize<S: Serializer>(value: &f64, s: S) -> Result<S::Ok, S::Error> {
                finite::<S::Error>($round(*value))?.serialize(s)
            }

            /// Deserialises, then rounds.
            pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
                Ok($round(f64::deserialize(d)?))
            }
        }

        #[doc = concat!("Serde helper for an optional ", $what, ".")]
        pub mod $optional {
            use super::*;

            /// Rounds, then serialises.
            pub fn serialize<S: Serializer>(value: &Option<f64>, s: S) -> Result<S::Ok, S::Error> {
                match value {
                    Some(v) => Some(finite::<S::Error>($round(*v))?).serialize(s),
                    None => None::<f64>.serialize(s),
                }
            }

            /// Deserialises, then rounds.
            pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
                Ok(Option::<f64>::deserialize(d)?.map($round))
            }
        }

        #[doc = concat!("Serde helper for a list of ", $what, ".")]
        pub mod $list {
            use super::*;

            /// Rounds, then serialises.
            pub fn serialize<S: Serializer>(value: &[f64], s: S) -> Result<S::Ok, S::Error> {
                let rounded = value
                    .iter()
                    .map(|v| finite::<S::Error>($round(*v)))
                    .collect::<Result<Vec<f64>, S::Error>>()?;
                rounded.serialize(s)
            }

            /// Deserialises, then rounds.
            pub fn deserialize<'de, D: Deserializer<'de>>(d: D) -> Result<Vec<f64>, D::Error> {
                Ok(Vec::<f64>::deserialize(d)?
                    .into_iter()
                    .map($round)
                    .collect())
            }
        }

        #[doc = concat!("Serde helper for rows of optional ", $what, ".")]
        pub mod $optional_list {
            use super::*;

            /// Rounds, then serialises.
            pub fn serialize<S: Serializer>(
                value: &[Vec<Option<f64>>],
                s: S,
            ) -> Result<S::Ok, S::Error> {
                let mut rows = Vec::with_capacity(value.len());
                for row in value {
                    let mut out = Vec::with_capacity(row.len());
                    for cell in row {
                        out.push(match cell {
                            Some(v) => Some(finite::<S::Error>($round(*v))?),
                            None => None,
                        });
                    }
                    rows.push(out);
                }
                rows.serialize(s)
            }

            /// Deserialises, then rounds.
            pub fn deserialize<'de, D: Deserializer<'de>>(
                d: D,
            ) -> Result<Vec<Vec<Option<f64>>>, D::Error> {
                Ok(Vec::<Vec<Option<f64>>>::deserialize(d)?
                    .into_iter()
                    .map(|row| row.into_iter().map(|c| c.map($round)).collect())
                    .collect())
            }
        }
    };
}

rounding_serde!(
    degrees_field,
    optional_degrees_field,
    degrees_list,
    optional_degrees_rows,
    degrees,
    "an angle or coordinate in degrees"
);
rounding_serde!(
    knots_field,
    optional_knots_field,
    knots_list,
    optional_knots_rows,
    knots,
    "a speed in knots"
);
rounding_serde!(
    metres_field,
    optional_metres_field,
    metres_list,
    optional_metres_rows,
    metres,
    "a length in metres"
);
rounding_serde!(
    ratio_field,
    optional_ratio_field,
    ratio_list,
    optional_ratio_rows,
    ratio,
    "a dimensionless value"
);

#[cfg(test)]
mod tests {
    use super::*;

    /// The property the whole module exists to guarantee, checked against
    /// `std`'s parser, which is always correctly rounded and is not the one
    /// under test.
    #[test]
    fn rounded_values_are_named_exactly_by_their_own_text() {
        let mut state: u64 = 0x243f_6a88_85a3_08d3;
        for _ in 0..20_000 {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            let unit = state as f64 / u64::MAX as f64;
            for (value, rounder) in [
                (unit * 360.0 - 180.0, degrees as fn(f64) -> f64),
                (unit * 60.0, knots),
                (unit * 1.0e4, metres),
                (unit * 2.0, ratio),
            ] {
                let canonical = rounder(value);
                let json = serde_json::to_string(&canonical).unwrap();
                let by_std: f64 = json.parse().unwrap();
                assert_eq!(by_std.to_bits(), canonical.to_bits(), "{json}");
                let by_serde: f64 = serde_json::from_str(&json).unwrap();
                assert_eq!(rounder(by_serde).to_bits(), canonical.to_bits(), "{json}");
            }
        }
    }

    #[test]
    fn rounding_is_idempotent_and_close() {
        for v in [
            0.0,
            -1.5,
            1_234.567_891_234,
            -98_765.432_1,
            179.999_999_999_9,
        ] {
            assert_eq!(degrees(degrees(v)).to_bits(), degrees(v).to_bits());
            assert_eq!(knots(knots(v)).to_bits(), knots(v).to_bits());
            assert!((degrees(v) - v).abs() <= 5e-10);
        }
    }

    #[test]
    fn environment_values_round_coarsely_and_stay_in_range() {
        assert_eq!(env_knots(12.345_678), 12.35);
        assert_eq!(env_metres(1.234_9), 1.23);
        assert_eq!(env_degrees(359.96), 0.0);
        assert_eq!(env_degrees(-0.04), 0.0);
        assert_eq!(env_degrees(-10.0), 350.0);
        assert_eq!(env_degrees(123.456), 123.5);
        for v in [0.049, 17.25, 359.94] {
            assert_eq!(env_degrees(env_degrees(v)), env_degrees(v));
            assert_eq!(env_knots(env_knots(v)), env_knots(v));
        }
    }

    #[test]
    fn negative_zero_is_written_as_zero() {
        assert_eq!(knots(-0.000_000_1).to_bits(), 0.0f64.to_bits());
        assert_eq!(degrees(-0.0).to_bits(), 0.0f64.to_bits());
    }

    #[test]
    fn extreme_and_non_finite_values_pass_through_rounding() {
        assert!(round(f64::NAN, 3).is_nan());
        assert_eq!(round(f64::INFINITY, 3), f64::INFINITY);
        assert_eq!(round(1e300, 3), 1e300);
    }

    #[derive(Debug, serde::Serialize)]
    struct Holder {
        #[serde(with = "knots_field")]
        speed: f64,
    }

    /// A NaN must not become `null` in a file that then refuses to open.
    #[test]
    fn a_non_finite_value_is_refused_rather_than_written_as_null() {
        let err = serde_json::to_string(&Holder { speed: f64::NAN }).unwrap_err();
        assert!(err.to_string().contains("only finite numbers"), "{err}");
        assert_eq!(
            serde_json::to_string(&Holder {
                speed: 6.123_456_789
            })
            .unwrap(),
            r#"{"speed":6.123457}"#
        );
    }
}
