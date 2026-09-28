//! GRIB2 writer for reanalysis export.
//!
//! A fixed-layout message template, as in VectorEffects' `ve-grib`, over
//! regional grids (spec.md 7.8). Output is byte-reproducible (invariant 5)
//! and is checked in CI against an independent decoder.
//!
//! Filled in by milestone M16 (plan.md).
