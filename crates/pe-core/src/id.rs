//! Identifiers the project allocates.
//!
//! Every id comes from the project's own `next_id` counter, so ids are stable
//! across save and load and **never reused**: a history entry, an exclusion
//! list or a later import can name a source or sample without holding it,
//! and a deleted one cannot come back as something else. Undo does not return
//! the counter, for the same reason.

use serde::{Deserialize, Serialize};

macro_rules! id_type {
    ($name:ident, $doc:literal) => {
        #[doc = $doc]
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(pub u64);

        impl $name {
            /// The raw value.
            pub const fn raw(self) -> u64 {
                self.0
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "#{}", self.0)
            }
        }
    };
}

id_type!(SourceId, "A source in the project's source list.");
id_type!(TrackId, "A track: names its `tracks/<id>.json` entry.");
id_type!(SampleId, "One track position, as excluded by an overlay.");

/// A project's identity, independent of its file name.
///
/// Names its crash-recovery snapshot (spec.md 4.5), so two projects open one
/// after the other must not share one. Chosen at creation from the clock and a
/// process counter, then stored in the file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ProjectId(pub u64);

impl ProjectId {
    /// A fresh id, distinct from every other one made by this process and,
    /// in practice, by any earlier run.
    pub fn fresh() -> Self {
        use std::sync::atomic::{AtomicU64, Ordering};
        static LAST: AtomicU64 = AtomicU64::new(0);
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_micros() as u64);
        let previous = LAST
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| {
                Some(last.max(now).saturating_add(1))
            })
            .unwrap_or(now);
        // Kept below 2^53 so it survives as a JavaScript number over IPC.
        (previous.max(now).saturating_add(1) & ((1 << 53) - 1)).into()
    }

    /// The raw value.
    pub const fn raw(self) -> u64 {
        self.0
    }
}

impl From<u64> for ProjectId {
    fn from(raw: u64) -> Self {
        Self(raw)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_project_ids_differ() {
        let a = ProjectId::fresh();
        let b = ProjectId::fresh();
        assert_ne!(a, b);
        assert!(b.raw() < (1 << 53));
    }

    #[test]
    fn ids_serialise_as_bare_numbers() {
        assert_eq!(serde_json::to_string(&SourceId(7)).unwrap(), "7");
        assert_eq!(serde_json::from_str::<SampleId>("9").unwrap(), SampleId(9));
    }
}
