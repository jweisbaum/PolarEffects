//! Error taxonomy for `pe-grib`. Ported from VectorEffects' `ve-grib`.

use thiserror::Error;

/// Errors produced while encoding GRIB2 output.
#[derive(Debug, Error)]
pub enum GribError {
    /// Writing the output file failed.
    #[error("the GRIB file could not be written: {0}")]
    Io(#[from] std::io::Error),

    /// A field contained a non-finite value, which cannot be packed.
    #[error(
        "the field held a value that is not a number, at grid point {0}; a NaN or an infinity cannot be encoded"
    )]
    NonFiniteValue(usize),

    /// The requested grid, time or field does not fit the template.
    #[error("unsupported grid: {0}")]
    UnsupportedGrid(String),
}

/// Convenience alias for results in this crate.
pub type Result<T> = std::result::Result<T, GribError>;
