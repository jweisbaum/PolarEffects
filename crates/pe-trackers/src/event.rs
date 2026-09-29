//! The shape every tracker's event takes, and the trait each tracker
//! implements (spec.md 7.2, CLAUDE.md "Adding a tracker").
//!
//! All three trackers share one dialog flow: resolve the pasted address to
//! an event, download **every** boat's full track at once, then let the user
//! pick boats. So a client's one network call, [`TrackerClient::fetch`],
//! returns the whole [`TrackerEvent`]; listing its boats and taking one
//! boat's fixes are then reads of that value, which the app keeps for the
//! session so a second import from the same event downloads nothing.
//!
//! A tracker that names its boats in a response of its own (YellowBrick's
//! RaceSetup, Geovoile's config) hands that list over as soon as it has
//! it, through [`TrackerClient::fetch_listed`], while the positions still
//! download: the dialog shows a pickable table at once (D24).

use pe_core::track::{Fix, Tracker};

use crate::error::Result;
use crate::http::Fetcher;

/// An event a pasted address names, before anything is downloaded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EventRef {
    /// Which tracker.
    pub tracker: Tracker,
    /// The tracker's own key for the event, e.g. `fastnet2025`.
    pub key: String,
    /// The event's address in its canonical form, as the track records it.
    pub url: String,
}

/// One boat of an event, with its full track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackerBoat {
    /// The tracker's own id for the boat.
    pub id: String,
    /// Boat name.
    pub name: String,
    /// Sail number.
    pub sail: Option<String>,
    /// Model or class.
    pub model: Option<String>,
    /// Division or class within the event, as the tracker groups it.
    pub division: Option<String>,
    /// The tracker's status, e.g. `RACING`, `FINISHED`, `RETIRED`.
    pub status: Option<String>,
    /// This boat's start, when the tracker gives one, else the event's.
    pub start: Option<i64>,
    /// This boat's finish, when the tracker gives one, else the event's end.
    pub finish: Option<i64>,
    /// Positions, oldest first, longitude in [-180, 180).
    pub fixes: Vec<Fix>,
}

/// Where an event's positions were read from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PositionsFrom {
    /// The tracker's primary format.
    Primary,
    /// A fallback, because the primary one did not decode (YellowBrick's
    /// KML, spec.md 7.2).
    Fallback,
}

/// A downloaded event: its title and dates and every boat's track.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackerEvent {
    /// Which event.
    pub event: EventRef,
    /// Title as the tracker gives it.
    pub title: String,
    /// Event start, UTC epoch seconds.
    pub start: Option<i64>,
    /// Event end, UTC epoch seconds.
    pub stop: Option<i64>,
    /// Every boat, in the tracker's order.
    pub boats: Vec<TrackerBoat>,
    /// Which format the positions came from.
    pub positions_from: PositionsFrom,
    /// For one leg of a race sailed in legs: `(leg, legs)`, from 1.
    pub leg: Option<(u32, u32)>,
}

impl TrackerEvent {
    /// The boats (the dialog's table).
    pub fn boats(&self) -> &[TrackerBoat] {
        &self.boats
    }

    /// One boat by the tracker's id.
    pub fn boat(&self, id: &str) -> Option<&TrackerBoat> {
        self.boats.iter().find(|b| b.id == id)
    }

    /// One boat's fixes, oldest first.
    pub fn fixes(&self, id: &str) -> Option<&[Fix]> {
        self.boat(id).map(|b| b.fixes.as_slice())
    }
}

/// How far a download is, for the job's progress (spec.md 7.7).
///
/// A download's steps are fixed when it starts, so the fraction only ever
/// grows: a tracker with a fallback counts the fallback's step from the
/// start and, when it is not needed, reports that step done at the end.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Progress {
    /// Which step: 0-based.
    pub step: u32,
    /// How many steps the download has, a possible fallback's included.
    pub steps: u32,
    /// Bytes of this step's response read so far.
    pub bytes: u64,
    /// The response's announced length, when the server gave one.
    pub total: Option<u64>,
    /// Whether this step reads the fallback format (YellowBrick's KML).
    pub fallback: bool,
}

impl Progress {
    /// Overall 0–1, the steps weighted equally; a step without a length
    /// counts as half done once it has bytes.
    pub fn fraction(&self) -> f64 {
        let within = match self.total {
            Some(total) if total > 0 => (self.bytes as f64 / total as f64).min(1.0),
            _ if self.bytes > 0 => 0.5,
            _ => 0.0,
        };
        let steps = f64::from(self.steps.max(1));
        ((f64::from(self.step) + within) / steps).clamp(0.0, 1.0)
    }
}

/// A race tracker (CLAUDE.md "Adding a tracker").
pub trait TrackerClient: Send + Sync {
    /// Which tracker this is.
    fn tracker(&self) -> Tracker;

    /// The event a pasted address names. No network.
    ///
    /// # Errors
    /// [`crate::TrackerError::NotAnEvent`] for an address this tracker does
    /// not serve.
    fn resolve(&self, input: &str) -> Result<EventRef>;

    /// Downloads the event: its title, dates and every boat's full track.
    ///
    /// # Errors
    /// Whatever the download or the decoder reports; cancelled when the
    /// fetcher's flag is set.
    fn fetch(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
    ) -> Result<TrackerEvent> {
        self.fetch_listed(event, fetcher, progress, &mut |_| {})
    }

    /// [`TrackerClient::fetch`], also calling `listed` once with the event's
    /// title, dates and boats, **without fixes**, as soon as they are known
    /// and before the positions are read, when the tracker gives them
    /// separately. A tracker that answers everything at once (Blue Water
    /// Tracks) never calls it.
    ///
    /// # Errors
    /// As [`TrackerClient::fetch`].
    fn fetch_listed(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        listed: &mut dyn FnMut(TrackerEvent),
    ) -> Result<TrackerEvent>;
}

/// The client of a tracker, when this build has one.
pub fn client(tracker: Tracker) -> Option<Box<dyn TrackerClient>> {
    match tracker {
        Tracker::YellowBrick => Some(Box::new(crate::yellowbrick::YellowBrick::default())),
        Tracker::Geovoile => Some(Box::new(crate::geovoile::Geovoile::default())),
        Tracker::BlueWaterTracks => Some(Box::new(crate::bluewater::BlueWaterTracks::default())),
    }
}

/// The name a tracker goes by in messages.
pub fn name(tracker: Tracker) -> &'static str {
    match tracker {
        Tracker::YellowBrick => "YellowBrick",
        Tracker::Geovoile => "Geovoile",
        Tracker::BlueWaterTracks => "Blue Water Tracks",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn progress_weighs_steps_equally() {
        let p = |step, bytes, total| Progress {
            step,
            steps: 2,
            bytes,
            total,
            fallback: false,
        };
        assert_eq!(p(0, 0, None).fraction(), 0.0);
        assert_eq!(p(0, 50, Some(100)).fraction(), 0.25);
        assert_eq!(p(1, 100, Some(100)).fraction(), 1.0);
        assert_eq!(p(1, 10, None).fraction(), 0.75);
    }
}
