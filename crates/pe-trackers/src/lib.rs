//! Race tracker clients: YellowBrick, Geovoile and Blue Water Tracks.
//!
//! One of the two crates allowed to use the network (invariant 4), and only
//! for its allow-listed hosts, which `tools/check-offline.sh` enforces. Each
//! tracker resolves a race URL to an event, lists its boats and fetches one
//! boat's fixes, decoding the vendor format in Rust (spec.md 7.2). Requests
//! are made only when the user asks for an import.
//!
//! Every tracker implements [`event::TrackerClient`]: resolve a pasted
//! address to an event (no network), then download the whole event — its
//! title, dates and every boat's full track — as one [`event::TrackerEvent`],
//! through one [`http::Fetcher`] that caps bodies, retries transient
//! failures, stops on cancel and follows redirects only within the
//! allow-list ([`net::redirect_allowed`]). YellowBrick (M10) and Geovoile
//! (M11) are complete; Blue Water Tracks follows in M12.

pub mod error;
pub mod event;
pub mod geovoile;
pub mod http;
pub mod kml;
pub mod net;
pub mod yellowbrick;

pub use error::{Result, TrackerError};
pub use event::{EventRef, Progress, TrackerBoat, TrackerClient, TrackerEvent};
pub use http::Fetcher;

/// Longitude in [-180, 180) (CLAUDE.md conventions), touching only values
/// outside it so a recorded value keeps its exact decimal form.
pub(crate) fn wrap_lon(lon: f64) -> f64 {
    if (-180.0..180.0).contains(&lon) {
        lon
    } else {
        (lon + 180.0).rem_euclid(360.0) - 180.0
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn longitudes_wrap_into_the_convention_and_keep_their_digits() {
        assert_eq!(super::wrap_lon(-1.22361), -1.22361);
        assert_eq!(super::wrap_lon(180.0), -180.0);
        assert_eq!(super::wrap_lon(359.5), -0.5);
        assert_eq!(super::wrap_lon(-180.5), 179.5);
        assert_eq!(super::wrap_lon(720.0 + 10.0), 10.0);
        assert_eq!(super::wrap_lon(-900.0), 180.0 - 360.0);
    }
}
