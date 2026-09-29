//! GRIB2 writer for reanalysis export (spec.md 7.8, D14).
//!
//! A fixed-layout message template ported from VectorEffects' `ve-grib`
//! (`writer`, `packing`), with regional template 3.0 grids across the
//! antimeridian (`region`) and a file written message by message and
//! renamed into place (`file`). Output is byte-reproducible (invariant 5):
//! integer arithmetic for the grid, a fixed section layout, and no clock
//! or host in any byte. It is checked by the in-repo `reader` (the
//! `testing` feature) and, in CI, by ecCodes.
//!
//! Longitudes are 0–360 inside this crate (CLAUDE.md conventions); the grid
//! hands the application its points in [−180, 180).

pub mod error;
pub mod file;
pub mod packing;
#[cfg(feature = "testing")]
pub mod reader;
pub mod region;
pub mod writer;

pub use error::{GribError, Result};
pub use file::{GribFile, Written};
pub use region::{MARGIN_DEG, NATIVE_STEP, region, times};
pub use writer::{GridSpec, MessageSpec, Parameter, ReferenceTime};
