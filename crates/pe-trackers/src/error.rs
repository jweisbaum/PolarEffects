//! Error taxonomy for `pe-trackers`.

use thiserror::Error;

/// Errors produced while fetching or decoding a tracker's data.
#[derive(Debug, Error)]
pub enum TrackerError {
    /// The request could not be made or the tracker answered an error.
    #[error("{0}")]
    Network(String),

    /// The URL or key does not name an event this tracker serves.
    #[error("not a {tracker} event address: {input}")]
    NotAnEvent {
        /// Which tracker.
        tracker: &'static str,
        /// What the user pasted.
        input: String,
    },

    /// A response did not decode. `at` names the byte offset or place.
    #[error("{what} did not decode at {at}: {why}")]
    Decode {
        /// Which response.
        what: &'static str,
        /// Where, e.g. "byte 1234" or "line 3".
        at: String,
        /// What was wrong.
        why: String,
    },

    /// The response decoded but is not in a format generation this build
    /// understands (a Flash-era Geovoile tracker, a changed encoding).
    #[error("unsupported {tracker} version: {why}")]
    Unsupported {
        /// Which tracker.
        tracker: &'static str,
        /// Why it was refused.
        why: String,
    },
}

/// Convenience alias for results in this crate.
pub type Result<T> = std::result::Result<T, TrackerError>;
