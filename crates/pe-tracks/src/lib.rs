//! Tracks and their samples.
//!
//! The track model, GeoJSON and CSV import, heading and speed derivation from
//! successive fixes, and the sample filters (spec.md 7). A track's raw fixes
//! are stored exactly as imported (invariant 1); derived values and filters
//! sit beside them.
//!
//! No network: tracker clients live in `pe-trackers`, which depends on this
//! crate and never the other way round.
//!
//! Filled in by milestone M8 (plan.md).
