//! Reanalysis environment at track positions.
//!
//! Zarr readers for WeatherBench2 wind, ARCO-ERA5 waves and ocean currents,
//! a pure-Rust Blosc/LZ4 decoder, and space-time sampling (spec.md 7.5).
//! Only the blocks of a chunk that hold a track's rows are downloaded
//! (`dataset.rs`), and they are kept in memory for the session
//! (`memory.rs`), never on disk: a project saves the values interpolated at
//! its samples, with their dataset name and version (invariant 3).
//!
//! One of the two crates allowed to use the network (invariant 4), and only
//! for the archive hosts that `tools/check-offline.sh` allows. Nothing here
//! runs unless the user asked for an import or a refetch.
//!
//! The store, codec and HTTP layers are ported from VectorEffects' `ve-zarr`
//! (M3 spike, kept as the foundation for M9).

pub mod blosc;
pub mod codec;
pub mod dataset;
pub mod error;
pub mod grid;
pub mod http;
pub mod memory;
pub mod net;
pub mod parallel;
pub mod sampler;
pub mod store;
pub mod time;

pub use dataset::{Dataset, OpenVariable, Variable, vars};
pub use error::{EnvError, Result};
pub use memory::BlockCache;
pub use sampler::{
    Access, EnvPoint, Estimate, ExportEstimate, Interval, Options, Parts, Point, Provider,
    Reanalysis, Vector, Waves, estimate, estimate_export,
};
