//! Blue Water Tracks (spec.md 7.2).
//!
//! One public response answers a whole event: `GET https://api.
//! bluewatertracks.com/api/race/<slug>` gives `race` (its name, start and
//! track window, and every boat) and `positions` (every boat's fixes, as
//! GeoJSON `Point` features with `sog` and `cog` in their properties). No
//! credential is needed.
//!
//! An unknown slug is not a 404: the API answers HTTP 200 with
//! `{"positions":[],"race":[]}` — `race` an empty array rather than the
//! object a real event gives — so that is checked before the object is
//! read, alongside the ordinary 404 a redirect or a changed API might still
//! give.
//!
//! The positions are not guaranteed sorted or free of duplicate timestamps
//! per boat (unlike YellowBrick's and Geovoile's own formats, which give
//! each boat's history in one run); they are put through the same
//! [`pe_tracks::normalise`] a file import uses, so [`event::TrackerBoat`]'s
//! "oldest first" holds here exactly as it does for every other tracker.

use std::collections::BTreeMap;

use serde::Deserialize;

use pe_core::track::{Fix, Tracker};

use crate::error::{Result, TrackerError};
use crate::event::{EventRef, PositionsFrom, Progress, TrackerBoat, TrackerClient, TrackerEvent};
use crate::http::Fetcher;

const TRACKER: &str = "Blue Water Tracks";

/// The public site a pasted address names, and the event's canonical form.
pub const SITE: &str = "https://race.bluewatertracks.com";

/// The API host an event is read from.
pub const API: &str = "https://api.bluewatertracks.com";

/// Whether `slug` is safe to put in a request path as it is: 1–128 of
/// `[A-Za-z0-9_-]`, which is the shape every slug in the race list has.
fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
}

/// The slug in a pasted address: `https://race.bluewatertracks.com/<slug>`
/// (the slug is the last path component), an
/// `api.bluewatertracks.com/api/race/<slug>` link, or a bare slug.
///
/// # Errors
/// [`TrackerError::NotAnEvent`] for another host or no usable slug.
pub fn race_slug(input: &str) -> Result<String> {
    let not_an_event = || TrackerError::NotAnEvent {
        tracker: TRACKER,
        input: input.to_owned(),
    };
    let text = input.trim();
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"));
    let rest = match rest {
        Some(rest) => rest,
        // A bare slug.
        None if valid_slug(text) => return Ok(text.to_owned()),
        // A host and path without a scheme.
        None if text.contains('/') => text,
        None => return Err(not_an_event()),
    };
    let (host_path, _query) = rest.split_once('?').unwrap_or((rest, ""));
    let (host, path) = host_path.split_once('/').unwrap_or((host_path, ""));
    let host = host.to_ascii_lowercase();
    let segments: Vec<&str> = path
        .split(['/', '#'])
        .filter(|s| !s.is_empty() && *s != "index.html")
        .collect();
    let slug = match host.as_str() {
        "race.bluewatertracks.com" => segments.last().copied(),
        "api.bluewatertracks.com" => match segments.as_slice() {
            [.., "race", slug] => Some(*slug),
            _ => None,
        },
        _ => None,
    };
    match slug.filter(|s| valid_slug(s)) {
        Some(slug) => Ok(slug.to_owned()),
        None => Err(not_an_event()),
    }
}

/// One rating system's entry for a boat.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, serde::Serialize)]
pub struct Handicap {
    /// The rating system's name, e.g. `"IRC"`.
    #[serde(default)]
    pub name: Option<String>,
    /// The rating, as the tracker writes it (not always a plain number).
    #[serde(default)]
    pub rating: Option<String>,
    /// The division this rating groups the boat into.
    #[serde(default)]
    pub division: Option<String>,
}

/// One boat as `race.boats[]` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct Boat {
    /// Preserve vendor metadata not needed by the position decoder.
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
    /// The id `positions[].properties.boat_id` names the boat by.
    pub boat_id: String,
    /// Boat name.
    #[serde(default, rename = "boatName")]
    pub boat_name: String,
    /// Sail number.
    #[serde(default, rename = "sailNo")]
    pub sail_no: Option<String>,
    /// Design or model, e.g. `"Lidgard 36"`.
    #[serde(default)]
    pub design: Option<String>,
    /// Every rating system's entry.
    #[serde(default)]
    pub handicaps: Vec<Handicap>,
    /// The tracker's status, e.g. `"Racing"`, when given.
    #[serde(default)]
    pub status: Option<String>,
    /// This boat's finish, ISO 8601, when given.
    #[serde(default, rename = "finishTime")]
    pub finish_time: Option<String>,
    /// Hull type, e.g. `"monohull"`. Kept for completeness; not shown.
    #[serde(default, rename = "type")]
    pub hull: Option<String>,
    /// Length, as written (units vary by `units`). Kept for completeness;
    /// not shown.
    #[serde(default)]
    pub length: Option<String>,
}

/// The parts of `race` an import uses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RaceInfo {
    /// Event title.
    #[serde(default, rename = "raceName")]
    pub race_name: String,
    /// Race start, ISO 8601, when given.
    #[serde(default, rename = "raceStartTime")]
    pub race_start_time: Option<String>,
    /// The end of the tracked window, ISO 8601, when given: the default
    /// finish for a boat the tracker gives no `finishTime` for.
    #[serde(default, rename = "trackTimeFinish")]
    pub track_time_finish: Option<String>,
    /// Every boat.
    #[serde(default)]
    pub boats: Vec<Boat>,
}

/// A `Point` feature of `positions[]`.
#[derive(Debug, Clone, PartialEq, Default, Deserialize)]
struct Geometry {
    /// `[lon, lat]` or `[lon, lat, altitude]`; the third value, when given,
    /// is ignored (spec.md 7.2 research notes).
    #[serde(default)]
    coordinates: Vec<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct PositionProps {
    boat_id: String,
    /// ISO 8601.
    date: String,
    /// Knots, given and used as is: unlike Geovoile's official reports, a
    /// value of exactly 0 is not a "none" sentinel here.
    #[serde(default)]
    sog: Option<f64>,
    /// Degrees, given and used as is.
    #[serde(default)]
    cog: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
struct PositionFeature {
    #[serde(default)]
    geometry: Geometry,
    properties: PositionProps,
}

/// The response `GET /api/race/<slug>` gives for a real event: `race` an
/// object (an unknown slug's `race` is an empty array instead, checked
/// before this is parsed) and every boat's positions, in no particular
/// order.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RaceResponse {
    /// The race.
    pub race: RaceInfo,
    /// Every boat's fixes, unsorted and possibly with duplicate timestamps.
    #[serde(default)]
    positions: Vec<PositionFeature>,
}

fn decode_error(why: impl Into<String>) -> TrackerError {
    TrackerError::Decode {
        what: "Blue Water Tracks race",
        at: "the response".to_owned(),
        why: why.into(),
    }
}

/// Parses `GET /api/race/<slug>`'s response, or `None` for an unknown
/// slug's answer: `{"positions":[],"race":[]}`, `race` an empty array
/// rather than the object a real event gives (spec.md 7.2 research notes).
///
/// # Errors
/// [`TrackerError::Decode`] for malformed JSON or a shape not matching
/// [`RaceResponse`].
pub fn parse_race(bytes: &[u8]) -> Result<Option<RaceResponse>> {
    let raw: serde_json::Value = serde_json::from_slice(bytes)
        .map_err(|e| decode_error(format!("line {} column {}: {e}", e.line(), e.column())))?;
    if !raw.get("race").is_some_and(serde_json::Value::is_object) {
        return Ok(None);
    }
    serde_json::from_value(raw)
        .map(Some)
        .map_err(|e| decode_error(e.to_string()))
}

fn iso(text: &str) -> Option<i64> {
    pe_tracks::time::parse_time(text, &pe_tracks::time::TimeFormat::Iso8601).ok()
}

fn non_empty(v: &Option<String>) -> Option<String> {
    v.as_ref()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}

/// A boat's division: the distinct `division` values across its handicaps,
/// in the order first seen, joined (spec.md 7.2). Every rating system
/// usually agrees, but a boat scored differently under two systems shows
/// both, the same way YellowBrick's division joins several starting tags.
fn division(handicaps: &[Handicap]) -> Option<String> {
    let mut seen: Vec<&str> = Vec::new();
    for h in handicaps {
        let Some(d) = h
            .division
            .as_deref()
            .map(str::trim)
            .filter(|d| !d.is_empty())
        else {
            continue;
        };
        if !seen.contains(&d) {
            seen.push(d);
        }
    }
    (!seen.is_empty()).then(|| seen.join(", "))
}

/// Parses one `Point` feature to `(boat id, fix)`.
///
/// # Errors
/// [`TrackerError::Decode`] naming the feature: a position needs a
/// longitude and a latitude, and a plausible time.
fn fix_of(at: usize, feature: &PositionFeature) -> Result<(String, Fix)> {
    let boat = feature.properties.boat_id.clone();
    let bad = |why: String| decode_error(format!("feature {at} (boat {boat:?}): {why}"));
    let &[lon, lat, ..] = feature.geometry.coordinates.as_slice() else {
        return Err(bad("a position needs a longitude and a latitude".to_owned()));
    };
    let Some(t) = iso(&feature.properties.date) else {
        return Err(bad(format!(
            "{:?} is not an ISO 8601 time",
            feature.properties.date
        )));
    };
    Ok((
        boat,
        Fix {
            tws: None,
            twd_from: None,
            t,
            lat,
            lon: crate::wrap_lon(lon),
            cog: feature.properties.cog,
            sog: feature.properties.sog,
        },
    ))
}

/// Builds the event from the response: every boat in `race.boats` order,
/// its fixes sorted and merged as a file import's are (spec.md 7.2, 7.4);
/// positions naming a boat `race.boats` does not list are left out, as the
/// other trackers leave out what their own setup does not list.
fn event_of(event: &EventRef, response: RaceResponse) -> Result<TrackerEvent> {
    let mut fixes_by_boat: BTreeMap<String, Vec<Fix>> = BTreeMap::new();
    for (at, feature) in response.positions.iter().enumerate() {
        let (boat, fix) = fix_of(at, feature)?;
        fixes_by_boat.entry(boat).or_default().push(fix);
    }
    let start = response.race.race_start_time.as_deref().and_then(iso);
    let stop = response.race.track_time_finish.as_deref().and_then(iso);
    let boats = response
        .race
        .boats
        .iter()
        .map(|boat| {
            let (fixes, _normalised) =
                pe_tracks::normalise(fixes_by_boat.remove(&boat.boat_id).unwrap_or_default());
            TrackerBoat {
                details: crate::event::boat_details(
                    &serde_json::to_value(boat).unwrap_or_default(),
                ),
                id: boat.boat_id.clone(),
                name: boat.boat_name.trim().to_owned(),
                sail: non_empty(&boat.sail_no),
                model: non_empty(&boat.design),
                division: division(&boat.handicaps),
                status: non_empty(&boat.status),
                start,
                finish: boat.finish_time.as_deref().and_then(iso).or(stop),
                fixes,
            }
        })
        .collect();
    Ok(TrackerEvent {
        event: event.clone(),
        title: response.race.race_name.trim().to_owned(),
        start,
        stop,
        boats,
        positions_from: PositionsFrom::Primary,
        leg: None,
    })
}

/// The Blue Water Tracks client (spec.md 7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlueWaterTracks {
    site: String,
    api: String,
}

impl Default for BlueWaterTracks {
    fn default() -> Self {
        Self {
            site: SITE.to_owned(),
            api: API.to_owned(),
        }
    }
}

impl BlueWaterTracks {
    /// A client reading from another host: the fixture tests serve a
    /// recorded response from a local server. The address it resolves is
    /// still the public one, as the real client's is.
    pub fn at(api: &str) -> Self {
        Self {
            site: SITE.to_owned(),
            api: api.trim_end_matches('/').to_owned(),
        }
    }
}

impl TrackerClient for BlueWaterTracks {
    fn tracker(&self) -> Tracker {
        Tracker::BlueWaterTracks
    }

    fn fetch_for_scrape(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        now: i64,
    ) -> Result<crate::library::completion::ScrapeFetch> {
        // This API puts positions and metadata in one response; its race list
        // has start times only. Never persist that response unless completion
        // is verified. finishTime is authoritative despite a stale Racing flag.
        let mut event = self.fetch(event, fetcher, progress)?;
        for boat in &mut event.boats {
            if boat
                .details
                .get("finishTime")
                .and_then(|s| iso(s))
                .is_some_and(|t| t > 0 && t <= now)
            {
                boat.status = Some("FINISHED".into());
            }
        }
        Ok(crate::library::completion::ScrapeFetch::checked(event, now))
    }

    fn resolve(&self, input: &str) -> Result<EventRef> {
        let slug = race_slug(input)?;
        Ok(EventRef {
            tracker: Tracker::BlueWaterTracks,
            url: format!("{}/{slug}", self.site),
            key: slug,
        })
    }

    /// One request, `GET /api/race/<slug>`, so there is no boat list ahead
    /// of the positions. An unknown slug answers 200 with `race` an empty
    /// array rather than an object; a 404 is treated the same, in case a
    /// redirect or a changed API ever gives one.
    fn fetch_listed(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        _listed: &mut dyn FnMut(TrackerEvent),
    ) -> Result<TrackerEvent> {
        let slug = event.key.as_str();
        // The slug goes into the request path as it is.
        if !valid_slug(slug) {
            return Err(TrackerError::NotAnEvent {
                tracker: TRACKER,
                input: slug.to_owned(),
            });
        }
        let no_such = || TrackerError::NoSuchEvent {
            tracker: TRACKER,
            key: slug.to_owned(),
        };
        let bytes = fetcher
            .get(&format!("{}/api/race/{slug}", self.api), &mut |b, t| {
                progress(Progress {
                    step: 0,
                    steps: 1,
                    bytes: b,
                    total: t,
                    fallback: false,
                });
            })
            .map_err(|e| match e {
                TrackerError::Http { status: 404, .. } => no_such(),
                other => other,
            })?;
        let response = match parse_race(&bytes)? {
            Some(response) if !response.race.boats.is_empty() => response,
            _ => return Err(no_such()),
        };
        event_of(event, response)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugs_come_from_every_address_form() {
        for input in [
            "melbourne-hobart-2025",
            " melbourne-hobart-2025 ",
            "https://race.bluewatertracks.com/melbourne-hobart-2025",
            "http://race.bluewatertracks.com/melbourne-hobart-2025/",
            "race.bluewatertracks.com/melbourne-hobart-2025",
            "https://api.bluewatertracks.com/api/race/melbourne-hobart-2025",
        ] {
            assert_eq!(
                race_slug(input).expect(input),
                "melbourne-hobart-2025",
                "{input}"
            );
        }
    }

    #[test]
    fn other_hosts_and_junk_are_refused() {
        for input in [
            "https://example.invalid/melbourne-hobart-2025",
            "https://race.bluewatertracks.com/",
            "",
            "not a slug",
            "https://race.bluewatertracks.com.evil.invalid/x",
            "https://api.bluewatertracks.com/api/notrace/x",
        ] {
            assert!(race_slug(input).is_err(), "{input}");
        }
    }

    #[test]
    fn resolve_gives_the_canonical_address() {
        let event = BlueWaterTracks::default()
            .resolve("https://api.bluewatertracks.com/api/race/melbourne-hobart-2025")
            .expect("resolves");
        assert_eq!(event.key, "melbourne-hobart-2025");
        assert_eq!(
            event.url,
            "https://race.bluewatertracks.com/melbourne-hobart-2025"
        );
    }

    /// An unknown slug's `race` is an empty array, not an object.
    #[test]
    fn an_unknown_slugs_response_parses_to_none() {
        assert_eq!(
            parse_race(br#"{"positions":[],"race":[]}"#).expect("valid JSON"),
            None
        );
        let err = parse_race(b"{\n\"race\": }").expect_err("bad JSON");
        assert!(err.to_string().contains("line 2"), "{err}");
    }

    /// A key goes into the request path as it is, so a key `race_slug`
    /// would not produce is refused before any request.
    #[test]
    fn fetch_refuses_a_key_that_is_not_a_slug() {
        let fetcher = Fetcher::new(
            TRACKER,
            std::time::Duration::from_secs(1),
            Default::default(),
        )
        .expect("a client");
        for key in ["", "../race/x", "a b", &"k".repeat(129)] {
            let event = EventRef {
                tracker: Tracker::BlueWaterTracks,
                key: key.to_owned(),
                url: String::new(),
            };
            let err = BlueWaterTracks::default()
                .fetch(&event, &fetcher, &mut |_| {})
                .expect_err(key);
            assert!(
                matches!(err, TrackerError::NotAnEvent { .. }),
                "{key}: {err}"
            );
        }
    }

    fn feature(
        boat: &str,
        t: &str,
        lon: f64,
        lat: f64,
        sog: Option<f64>,
        cog: Option<f64>,
    ) -> PositionFeature {
        PositionFeature {
            geometry: Geometry {
                coordinates: vec![lon, lat, 12.0],
            },
            properties: PositionProps {
                boat_id: boat.to_owned(),
                date: t.to_owned(),
                sog,
                cog,
            },
        }
    }

    /// Out-of-order and duplicate fixes are sorted and merged per boat, as
    /// a file import's are: the duplicate's own SOG fills a gap the first
    /// of the pair left, and never overwrites a value the first already had.
    #[test]
    fn positions_are_sorted_and_merged_per_boat() {
        let response = RaceResponse {
            race: RaceInfo {
                race_name: "Test Race".to_owned(),
                race_start_time: Some("2025-12-27T02:30:00.000Z".to_owned()),
                track_time_finish: Some("2026-01-31T12:00:00.000Z".to_owned()),
                boats: vec![Boat {
                    extra: Default::default(),
                    boat_id: "b1".to_owned(),
                    boat_name: "Alien".to_owned(),
                    sail_no: Some("R880".to_owned()),
                    design: Some("Lidgard 36".to_owned()),
                    handicaps: vec![
                        Handicap {
                            name: Some("AMS".to_owned()),
                            rating: Some("0.895".to_owned()),
                            division: Some("1".to_owned()),
                        },
                        Handicap {
                            name: Some("ORC".to_owned()),
                            rating: Some("1.1236".to_owned()),
                            division: Some("1".to_owned()),
                        },
                    ],
                    status: Some("Racing".to_owned()),
                    finish_time: None,
                    hull: Some("monohull".to_owned()),
                    length: Some("10.9".to_owned()),
                }],
            },
            positions: vec![
                // Newest first, and a duplicate of the middle time whose
                // SOG the first of the pair lacks.
                feature(
                    "b1",
                    "2025-12-27T00:20:00Z",
                    145.0,
                    -38.0,
                    Some(5.0),
                    Some(90.0),
                ),
                feature("b1", "2025-12-27T00:10:00Z", 144.9, -37.9, None, Some(88.0)),
                feature("b1", "2025-12-27T00:10:00Z", 144.9, -37.9, Some(4.5), None),
                feature(
                    "b1",
                    "2025-12-27T00:00:00Z",
                    144.8,
                    -37.8,
                    Some(4.0),
                    Some(85.0),
                ),
            ],
        };
        let event = event_of(
            &EventRef {
                tracker: Tracker::BlueWaterTracks,
                key: "test".to_owned(),
                url: String::new(),
            },
            response,
        )
        .expect("builds");
        assert_eq!(event.boats.len(), 1);
        let boat = &event.boats[0];
        assert_eq!(boat.division.as_deref(), Some("1"));
        assert_eq!(boat.fixes.len(), 3, "the duplicate is merged");
        assert!(
            boat.fixes.windows(2).all(|w| w[0].t < w[1].t),
            "oldest first"
        );
        let middle = &boat.fixes[1];
        assert_eq!(
            middle.sog,
            Some(4.5),
            "the duplicate's own SOG fills the gap"
        );
        assert_eq!(
            middle.cog,
            Some(88.0),
            "the first of the pair's COG is kept"
        );
        // No boat finish given: the event's tracked end.
        assert_eq!(boat.finish, event.stop);
    }
}
