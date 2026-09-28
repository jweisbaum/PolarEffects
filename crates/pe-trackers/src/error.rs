//! Error taxonomy for `pe-trackers`.

use thiserror::Error;

/// Errors produced while fetching or decoding a tracker's data.
#[derive(Debug, Error)]
pub enum TrackerError {
    /// The request could not be made, or the tracker refused it in a way
    /// asking again would not change (a 4xx answer, an oversized body).
    #[error("{0}")]
    Network(String),

    /// A response's status was a permanent failure (a 4xx other than 429):
    /// the status is kept, not only the message, so a caller can react to a
    /// specific one — a 404 — without matching the message text (CLAUDE.md
    /// "Adding a tracker").
    #[error("{why}")]
    Http {
        /// The HTTP status code.
        status: u16,
        /// The full message, as [`TrackerError::Network`] would carry it.
        why: String,
    },

    /// The tracker did not answer usefully after the bounded retries: a 5xx
    /// or 429 answer, a timeout or a dropped connection (spec.md 7.2: "some
    /// keys return 5xx; the dialog says so and offers Retry").
    #[error("{tracker} is not answering: {why}")]
    Unavailable {
        /// Which tracker.
        tracker: &'static str,
        /// The last failure.
        why: String,
    },

    /// The tracker has no public event under this key.
    #[error("{tracker} has no public event {key:?}")]
    NoSuchEvent {
        /// Which tracker.
        tracker: &'static str,
        /// The key asked for.
        key: String,
    },

    /// The user cancelled the download.
    #[error("the download was cancelled")]
    Cancelled,

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

    /// An older tracker generation this build does not read: a Flash
    /// Geovoile tracker (`.hwz`) or one of its 2012–2015 HTML trackers.
    #[error("this is an older {tracker} tracker, which is not supported: {why}")]
    Legacy {
        /// Which tracker.
        tracker: &'static str,
        /// What was recognised.
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
