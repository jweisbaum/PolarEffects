//! The PolarEffects document model.
//!
//! Everything a project *is* lives here: the `Project`, its sources and the
//! overlays that record every user change beside the immutable source data,
//! the ids the project allocates, the undo/redo command history, canonical
//! float serialisation, and `.wpsproj` reading, writing and migration
//! (spec.md 4).
//!
//! This crate depends on no sibling crate. Every other crate depends on it, so
//! the types that cross between them — a polar grid, an ORC record, a track
//! and its samples — are defined here once, as data. Their behaviour lives in
//! the crate that owns it (`pe-polar`, `pe-orc`, `pe-tracks`, `pe-env`).

pub mod canonical;
pub mod command;
pub mod error;
pub mod history;
pub mod id;
pub mod io;
pub mod orc;
pub mod polar;
pub mod project;
pub mod source;
pub mod track;

#[cfg(test)]
mod fixtures;

pub use command::Command;
pub use error::{CoreError, Result};
pub use history::History;
pub use id::{ProjectId, SampleId, SourceId, TrackId};
pub use project::{Boat, Project, SCHEMA_VERSION};
pub use source::{Colour, Overlay, Source, SourceKind};
