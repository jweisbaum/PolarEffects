//! The PolarEffects document model.
//!
//! Everything a project *is* lives here: the `Project`, its sources and the
//! overlays that record every user change beside the immutable source data,
//! the ids the project allocates, the undo/redo command history, spherical
//! geodesy, canonical float serialisation, and `.wpsproj` reading, writing and
//! migration (spec.md 4).
//!
//! This crate depends on no sibling crate. Every other crate depends on it, so
//! the types that cross between them are defined here once.
//!
//! Filled in by milestone M1 (plan.md).
