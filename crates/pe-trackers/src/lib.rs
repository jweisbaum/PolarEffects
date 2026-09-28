//! Race tracker clients: YellowBrick, Geovoile and Blue Water Tracks.
//!
//! One of the two crates allowed to use the network (invariant 4), and only
//! for its allow-listed hosts, which `tools/check-offline.sh` enforces. Each
//! tracker resolves a race URL to an event, lists its boats and fetches one
//! boat's fixes, decoding the vendor format in Rust (spec.md 7.2). Requests
//! are made only when the user asks for an import.
//!
//! Filled in from milestone M10 (plan.md).
