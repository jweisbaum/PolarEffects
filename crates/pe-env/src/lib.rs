//! Reanalysis environment at track positions.
//!
//! Zarr readers for WeatherBench2 wind, ARCO-ERA5 waves and ocean currents,
//! a pure-Rust Blosc/LZ4 decoder, the chunk cache, and space-time sampling
//! (spec.md 7.5). Samples are converted to project units on ingest and saved in
//! the project with their dataset name and version (invariant 3).
//!
//! One of the two crates allowed to use the network (invariant 4), and only
//! for the archive hosts that `tools/check-offline.sh` allows.
//!
//! Filled in by milestone M9 (plan.md).
