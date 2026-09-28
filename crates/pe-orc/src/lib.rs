//! The embedded ORC catalogue.
//!
//! The catalogue is generated at build time from a `jieter/orc-data` checkout
//! and embedded in the binary, with a search index over it (spec.md 5). It is
//! never fetched at run time (invariant 4), so this crate has no network code.
//!
//! Filled in by milestone M5 (plan.md).
