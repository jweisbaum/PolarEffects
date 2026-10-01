//! YellowBrick (spec.md 7.2, plan.md Appendix B).
//!
//! An event is two public responses: `RaceSetup` (JSON, ISO-8859-1: title,
//! start, stop and the teams) and `AllPositions3` (a big-endian binary with
//! every team's full history). Neither needs a credential. Catalogue code
//! discovery uses optional local credentials separately in library::yellowbrick.

use std::time::Duration;

use serde::Deserialize;

use pe_core::track::{Fix, Tracker};

use crate::error::{Result, TrackerError};
use crate::event::{EventRef, PositionsFrom, Progress, TrackerBoat, TrackerClient, TrackerEvent};
use crate::http::Fetcher;

/// The CDN host the public responses are served from.
pub const CDN: &str = "https://cf.yb.tl";

/// Hosts a pasted YellowBrick address may name.
const HOSTS: [&str; 4] = ["yb.tl", "www.yb.tl", "cf.yb.tl", "app.yb.tl"];

const TRACKER: &str = "YellowBrick";

/// Mobile catalogue codes include ampersands, dots and internal spaces.
/// Restrict them to one path segment and encode spaces before constructing URLs.
fn valid_key(key: &str) -> bool {
    !key.is_empty()
        && key.len() <= 64
        && key != "."
        && key != ".."
        && key
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '&' | '.' | ' '))
}

pub(crate) fn url_key(key: &str) -> String {
    // Every other accepted character is legal literally inside a URL path.
    key.replace(' ', "%20")
}
fn decoded_key(text: &str) -> Option<String> {
    let key = percent_encoding::percent_decode_str(text)
        .decode_utf8()
        .ok()?;
    valid_key(&key).then(|| key.into_owned())
}

/// The race key in a pasted address: `https://yb.tl/<key>`, a `cf.yb.tl`
/// or `app.yb.tl` link (including the `/JSON/<key>/…` and `/BIN/<key>/…`
/// forms and a `?race=<key>` query), or a bare key.
///
/// # Errors
/// [`TrackerError::NotAnEvent`] for another host or no usable key.
pub fn race_key(input: &str) -> Result<String> {
    let not_an_event = || TrackerError::NotAnEvent {
        tracker: TRACKER,
        input: input.to_owned(),
    };
    let text = input.trim();
    let valid = decoded_key;
    let rest = text
        .strip_prefix("https://")
        .or_else(|| text.strip_prefix("http://"));
    let rest = match rest {
        Some(rest) => rest,
        // A bare key.
        None if valid(text).is_some() => return valid(text).ok_or_else(not_an_event),
        // A host and path without a scheme.
        None if text.contains('/') => text,
        None => return Err(not_an_event()),
    };
    let (host_path, query) = rest.split_once('?').unwrap_or((rest, ""));
    let (host, path) = host_path.split_once('/').unwrap_or((host_path, ""));
    if !HOSTS.contains(&host.to_ascii_lowercase().as_str()) {
        return Err(not_an_event());
    }
    for pair in query.split('&') {
        if let Some(key) = pair.strip_prefix("race=")
            && let Some(key) = valid(key)
        {
            return Ok(key);
        }
    }
    let mut segments = path
        .split(['/', '#'])
        .filter(|s| !s.is_empty() && *s != "index.html");
    let first = segments.next().ok_or_else(not_an_event)?;
    let key = if first == "JSON" || first == "BIN" {
        segments.next().ok_or_else(not_an_event)?
    } else {
        first
    };
    valid(key).ok_or_else(not_an_event)
}

/// One team (boat) as `RaceSetup` lists it.
#[derive(Debug, Clone, PartialEq, Deserialize, serde::Serialize)]
pub struct Team {
    /// Unrecognized fields are retained for boat/model resolution.
    #[serde(flatten)]
    pub extra: std::collections::BTreeMap<String, serde_json::Value>,
    /// The id `AllPositions3` names the team by.
    pub id: u16,
    /// Boat name.
    #[serde(default)]
    pub name: String,
    /// Sail number.
    #[serde(default)]
    pub sail: Option<String>,
    /// Boat model or class.
    #[serde(default)]
    pub model: Option<String>,
    /// Owner, when given.
    #[serde(default)]
    pub owner: Option<String>,
    /// Hull type, e.g. `MONO_L`.
    #[serde(default, rename = "type")]
    pub hull: Option<String>,
    /// Division tags (ids into the setup's tag list).
    #[serde(default)]
    pub tags: Vec<i64>,
    /// `RACING`, `RETIRED`, `FINISHED`… when given.
    #[serde(default)]
    pub status: Option<String>,
    /// This team's own start (divisions start apart), UTC epoch seconds.
    #[serde(default)]
    pub start: Option<i64>,
    /// When this team finished, UTC epoch seconds.
    #[serde(default, rename = "finishedAt")]
    pub finished_at: Option<i64>,
}

/// A division or leaderboard tag.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct Tag {
    /// The id teams list in their `tags`.
    pub id: i64,
    /// Its name, e.g. "IRC 2".
    #[serde(default)]
    pub name: String,
    /// The tag's own start: set for the divisions that start separately.
    #[serde(default)]
    pub start: Option<i64>,
    /// Display order.
    #[serde(default)]
    pub sort: Option<i64>,
    /// 0 for a hidden tag.
    #[serde(default)]
    pub show: Option<i64>,
}

/// The parts of `RaceSetup` an import uses.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RaceSetup {
    /// Event title.
    #[serde(default)]
    pub title: String,
    /// Race start, UTC epoch seconds.
    #[serde(default)]
    pub start: Option<i64>,
    /// Race stop, UTC epoch seconds.
    #[serde(default)]
    pub stop: Option<i64>,
    /// Every team.
    #[serde(default)]
    pub teams: Vec<Team>,
    /// Divisions and leaderboards.
    #[serde(default)]
    pub tags: Vec<Tag>,
}

/// Parses `RaceSetup`, which is served as ISO-8859-1, not UTF-8
/// (CLAUDE.md "Environment gotchas"). Every Latin-1 byte is the Unicode
/// code point of the same value, so the bytes map to `char`s one to one.
///
/// # Errors
/// [`TrackerError::Decode`] naming the line and column of bad JSON.
pub fn parse_race_setup(bytes: &[u8]) -> Result<RaceSetup> {
    let text: String = bytes.iter().map(|&b| char::from(b)).collect();
    serde_json::from_str(&text).map_err(|e| TrackerError::Decode {
        what: "YellowBrick RaceSetup",
        at: format!("line {} column {}", e.line(), e.column()),
        why: e.to_string(),
    })
}

/// Which optional fields each moment carries.
const FLAG_ALT: u8 = 0x01;
const FLAG_DTF: u8 = 0x02;
const FLAG_LAP: u8 = 0x04;
const FLAG_PC: u8 = 0x08;

/// One position report.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Moment {
    /// UTC epoch seconds.
    pub at: i64,
    /// Latitude, degrees.
    pub lat: f64,
    /// Longitude, degrees.
    pub lon: f64,
    /// Altitude, when the event sets the flag. Layout unverified.
    pub alt: Option<i32>,
    /// Distance to finish, metres.
    pub dtf: Option<i64>,
    /// Lap number. Layout unverified.
    pub lap: Option<u8>,
    /// Percent complete. Layout unverified.
    pub pc: Option<i32>,
}

/// One team's history, **oldest first** (the file stores newest first).
#[derive(Debug, Clone, PartialEq)]
pub struct TeamTrack {
    /// `RaceSetup` team id.
    pub id: u16,
    /// Position reports, oldest first.
    pub moments: Vec<Moment>,
}

impl TeamTrack {
    /// The reports as track fixes, oldest first, longitude in [-180, 180).
    /// YellowBrick gives no course or speed; both are derived (spec.md 7.4).
    pub fn fixes(&self) -> Vec<Fix> {
        self.moments
            .iter()
            .map(|m| Fix {
                tws: None,
                twd_from: None,
                t: m.at,
                lat: m.lat,
                lon: crate::wrap_lon(m.lon),
                cog: None,
                sog: None,
            })
            .collect()
    }
}

/// A decoded `AllPositions3`.
#[derive(Debug, Clone, PartialEq)]
pub struct AllPositions {
    /// The header flags (bit0 altitude, bit1 DTF, bit2 lap, bit3 percent).
    pub flags: u8,
    /// Absolute times are offsets from this, UTC epoch seconds.
    pub ref_time: i64,
    /// Every team, in file order.
    pub teams: Vec<TeamTrack>,
}

/// A big-endian cursor that reports where it ran out.
struct Cursor<'a> {
    bytes: &'a [u8],
    at: usize,
}

impl Cursor<'_> {
    fn take<const N: usize>(&mut self) -> Result<[u8; N]> {
        let slice = self
            .bytes
            .get(self.at..self.at + N)
            .ok_or_else(|| decode_error(self.at, format!("needs {N} more bytes, the file ends")))?;
        self.at += N;
        let mut out = [0u8; N];
        out.copy_from_slice(slice);
        Ok(out)
    }
    fn u8(&mut self) -> Result<u8> {
        Ok(self.take::<1>()?[0])
    }
    fn u16(&mut self) -> Result<u16> {
        Ok(u16::from_be_bytes(self.take()?))
    }
    fn i16(&mut self) -> Result<i16> {
        Ok(i16::from_be_bytes(self.take()?))
    }
    fn u32(&mut self) -> Result<u32> {
        Ok(u32::from_be_bytes(self.take()?))
    }
    fn i32(&mut self) -> Result<i32> {
        Ok(i32::from_be_bytes(self.take()?))
    }
    fn peek(&self) -> Result<u8> {
        self.bytes
            .get(self.at)
            .copied()
            .ok_or_else(|| decode_error(self.at, "the file ends inside a team".to_owned()))
    }
}

fn decode_error(at: usize, why: String) -> TrackerError {
    TrackerError::Decode {
        what: "YellowBrick AllPositions3",
        at: format!("byte {at}"),
        why,
    }
}

/// Decodes `AllPositions3` (plan.md Appendix B).
///
/// Coordinates are integers in 1e-5 degrees; a moment with the high bit of
/// its first byte set is a delta from the previous, *newer* moment. The whole
/// file must be consumed exactly and every position must be on the globe:
/// anything else is an error naming the byte, never a partial result.
///
/// # Errors
/// [`TrackerError::Decode`] on truncation, a delta with nothing to be a
/// delta from, or an impossible position.
pub fn decode_all_positions(bytes: &[u8]) -> Result<AllPositions> {
    let mut c = Cursor { bytes, at: 0 };
    let flags = c.u8()?;
    if flags & !0x0F != 0 {
        return Err(TrackerError::Unsupported {
            tracker: TRACKER,
            why: format!("AllPositions3 header flags {flags:#04x} use unknown bits"),
        });
    }
    let ref_time = i64::from(c.u32()?);
    let mut teams = Vec::new();
    while c.at < bytes.len() {
        let id = c.u16()?;
        let count = c.u16()?;
        let mut newest_first: Vec<Moment> = Vec::with_capacity(usize::from(count));
        // Raw integer state of the previous moment, so deltas accumulate
        // exactly rather than through floating point.
        let mut prev: Option<(i64, i64, i64, Option<i64>)> = None;
        for _ in 0..count {
            let start = c.at;
            let absolute = c.peek()? & 0x80 == 0;
            let (at, lat, lon, alt, dtf, lap, pc);
            if absolute {
                at = ref_time + i64::from(c.u32()?);
                lat = i64::from(c.i32()?);
                lon = i64::from(c.i32()?);
                alt = if flags & FLAG_ALT != 0 {
                    Some(i32::from(c.i16()?))
                } else {
                    None
                };
                (dtf, lap) = if flags & FLAG_DTF != 0 {
                    let d = i64::from(c.i32()?);
                    (
                        Some(d),
                        if flags & FLAG_LAP != 0 {
                            Some(c.u8()?)
                        } else {
                            None
                        },
                    )
                } else {
                    (None, None)
                };
                pc = if flags & FLAG_PC != 0 {
                    Some(c.i32()?)
                } else {
                    None
                };
            } else {
                let (p_at, p_lat, p_lon, p_dtf) = prev.ok_or_else(|| {
                    decode_error(start, format!("team {id} starts with a delta moment"))
                })?;
                let w = c.u16()?;
                at = p_at - i64::from(w & 0x7FFF);
                lat = p_lat + i64::from(c.i16()?);
                lon = p_lon + i64::from(c.i16()?);
                alt = if flags & FLAG_ALT != 0 {
                    Some(i32::from(c.i16()?))
                } else {
                    None
                };
                (dtf, lap) = if flags & FLAG_DTF != 0 {
                    let d = i64::from(c.i16()?);
                    let base = p_dtf.unwrap_or(0);
                    (
                        Some(base + d),
                        if flags & FLAG_LAP != 0 {
                            Some(c.u8()?)
                        } else {
                            None
                        },
                    )
                } else {
                    (None, None)
                };
                pc = if flags & FLAG_PC != 0 {
                    Some(i32::from(c.i16()?))
                } else {
                    None
                };
            }
            let (lat_deg, lon_deg) = (lat as f64 / 1e5, lon as f64 / 1e5);
            if !(-90.0..=90.0).contains(&lat_deg) || !(-360.0..=360.0).contains(&lon_deg) {
                return Err(decode_error(
                    start,
                    format!(
                        "team {id} has a position at {lat_deg}, {lon_deg}, which is not on the globe"
                    ),
                ));
            }
            prev = Some((at, lat, lon, dtf));
            newest_first.push(Moment {
                at,
                lat: lat_deg,
                lon: lon_deg,
                alt,
                dtf,
                lap,
                pc,
            });
        }
        newest_first.reverse();
        teams.push(TeamTrack {
            id,
            moments: newest_first,
        });
    }
    Ok(AllPositions {
        flags,
        ref_time,
        teams,
    })
}

/// The public site, which serves the KML fallback. Viewer links use it too.
pub const SITE: &str = "https://yb.tl";

/// How long the KML fallback may take. YellowBrick builds it on request:
/// 73 s for the Middle Sea Race 2024 (23 MB), over two minutes for the
/// Fastnet 2025 (99 MB), so the per-request timeout would cut it short.
pub const KML_TIMEOUT: Duration = Duration::from_secs(600);

/// Whether a body is an HTML page rather than the JSON or binary asked for.
/// YellowBrick answers an unknown key's `AllPositions3` with its viewer page
/// and status 200.
fn is_html(bytes: &[u8]) -> bool {
    let start = bytes
        .iter()
        .position(|b| !b.is_ascii_whitespace())
        .unwrap_or(bytes.len());
    bytes[start..].starts_with(b"<")
}

impl RaceSetup {
    /// A team's division: the names of its tags that have a start of their
    /// own (the starting divisions, such as "IRC 2"), else of all its shown
    /// tags, in the setup's tag order.
    pub fn division(&self, team: &Team) -> Option<String> {
        let mut tags: Vec<&Tag> = self
            .tags
            .iter()
            .filter(|t| team.tags.contains(&t.id) && t.show != Some(0))
            .collect();
        tags.sort_by_key(|t| (t.sort.unwrap_or(i64::MAX), t.id));
        let starting: Vec<&str> = tags
            .iter()
            .filter(|t| t.start.is_some())
            .map(|t| t.name.as_str())
            .collect();
        let names: Vec<&str> = if starting.is_empty() {
            tags.iter().map(|t| t.name.as_str()).collect()
        } else {
            starting
        };
        (!names.is_empty()).then(|| names.join(", "))
    }
}

/// Builds the shared event from the setup and each team's fixes, in the
/// setup's team order. A team with no positions has no fixes; positions for
/// a team the setup does not list are left out.
fn event_of(
    event: &EventRef,
    setup: &RaceSetup,
    mut fixes: impl FnMut(&Team) -> Vec<Fix>,
    positions_from: PositionsFrom,
) -> TrackerEvent {
    let non_empty = |v: &Option<String>| {
        v.as_ref()
            .map(|s| s.trim().to_owned())
            .filter(|s| !s.is_empty())
    };
    let boats = setup
        .teams
        .iter()
        .map(|team| TrackerBoat {
            details: crate::event::boat_details(&serde_json::to_value(team).unwrap_or_default()),
            id: team.id.to_string(),
            name: team.name.trim().to_owned(),
            sail: non_empty(&team.sail),
            model: non_empty(&team.model),
            division: setup.division(team),
            status: if team.finished_at.is_some() {
                Some("FINISHED".to_owned())
            } else {
                non_empty(&team.status)
            },
            start: team.start.or(setup.start),
            finish: team.finished_at.or(setup.stop),
            fixes: fixes(team),
        })
        .collect();
    TrackerEvent {
        event: event.clone(),
        title: setup.title.trim().to_owned(),
        start: setup.start,
        stop: setup.stop,
        boats,
        positions_from,
        leg: None,
    }
}

/// The YellowBrick client (spec.md 7.2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct YellowBrick {
    cdn: String,
    site: String,
}

impl Default for YellowBrick {
    fn default() -> Self {
        Self {
            cdn: CDN.to_owned(),
            site: SITE.to_owned(),
        }
    }
}

impl YellowBrick {
    /// A client reading from other hosts: the fixture tests serve recorded
    /// responses from a local server.
    pub fn at(cdn: &str, site: &str) -> Self {
        Self {
            cdn: cdn.trim_end_matches('/').to_owned(),
            site: site.trim_end_matches('/').to_owned(),
        }
    }
}

impl TrackerClient for YellowBrick {
    fn tracker(&self) -> Tracker {
        Tracker::YellowBrick
    }

    fn fetch_for_scrape(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        now: i64,
    ) -> Result<crate::library::completion::ScrapeFetch> {
        if !valid_key(&event.key) {
            return Err(TrackerError::NotAnEvent {
                tracker: TRACKER,
                input: event.key.clone(),
            });
        }
        // Unlike the interactive import, never start AllPositions3 alongside
        // RaceSetup: an ongoing race must not download positions or KML.
        let bytes = fetcher.get(
            &format!("{}/JSON/{}/RaceSetup", self.cdn, url_key(&event.key)),
            &mut |_, _| {},
        )?;
        let setup = parse_race_setup(&bytes)?;
        let metadata = event_of(event, &setup, |_| Vec::new(), PositionsFrom::Primary);
        crate::library::completion::after_check(metadata, now, || {
            self.fetch(event, fetcher, progress)
        })
    }

    fn resolve(&self, input: &str) -> Result<EventRef> {
        let key = race_key(input)?;
        Ok(EventRef {
            tracker: Tracker::YellowBrick,
            url: format!("{SITE}/{}", url_key(&key)),
            key,
        })
    }

    /// `RaceSetup` and `AllPositions3` at the same time (the binary decoded
    /// on its own thread), the boat list handed to `listed` as soon as the
    /// setup is read; if the binary does not decode (or is refused), the
    /// KML instead. A cancel, or a tracker that keeps failing (5xx), ends
    /// the download without the fallback.
    fn fetch_listed(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        listed: &mut dyn FnMut(TrackerEvent),
    ) -> Result<TrackerEvent> {
        let key = event.key.as_str();
        // Validate before encoding the one path segment used by every request.
        if !valid_key(key) {
            return Err(TrackerError::NotAnEvent {
                tracker: TRACKER,
                input: key.to_owned(),
            });
        }
        // Three steps from the start: the setup, the binary, and the KML
        // fallback, reported done at once when the binary decodes, so the
        // fraction never goes back when the fallback is needed.
        let step = |step: u32| {
            move |bytes: u64, total: Option<u64>| Progress {
                step,
                steps: 3,
                bytes,
                total,
                fallback: step == 2,
            }
        };
        let no_such = || TrackerError::NoSuchEvent {
            tracker: TRACKER,
            key: key.to_owned(),
        };

        // The positions start at once: the setup is small and quick, and
        // the dialog lists the boats from it while the binary still reads.
        let binary = fetcher.spawn(
            format!("{}/BIN/{}/AllPositions3", self.cdn, url_key(key)),
            None,
            |bytes| {
                if is_html(&bytes) {
                    Err(decode_error(
                        0,
                        "the answer is a web page, not positions".to_owned(),
                    ))
                } else {
                    decode_all_positions(&bytes)
                }
            },
        )?;

        let at = step(0);
        let bytes = fetcher.get(
            &format!("{}/JSON/{}/RaceSetup", self.cdn, url_key(key)),
            &mut |b, t| {
                progress(at(b, t));
            },
        )?;
        if is_html(&bytes) {
            return Err(no_such());
        }
        let setup = parse_race_setup(&bytes)?;
        listed(event_of(
            event,
            &setup,
            |_| Vec::new(),
            PositionsFrom::Primary,
        ));

        let at = step(1);
        let binary = binary.wait(&mut |b, t| {
            progress(at(b, t));
        });
        let failure = match binary {
            Ok(all) => {
                progress(Progress {
                    step: 2,
                    steps: 3,
                    bytes: 1,
                    total: Some(1),
                    fallback: false,
                });
                let mut by_id: std::collections::BTreeMap<u16, TeamTrack> =
                    all.teams.into_iter().map(|t| (t.id, t)).collect();
                return Ok(event_of(
                    event,
                    &setup,
                    |team| {
                        by_id
                            .remove(&team.id)
                            .map(|t| t.fixes())
                            .unwrap_or_default()
                    },
                    PositionsFrom::Primary,
                ));
            }
            Err(e @ (TrackerError::Cancelled | TrackerError::Unavailable { .. })) => return Err(e),
            Err(e) => e,
        };

        // The fallback (spec.md 7.2): the same teams as a KML, by name.
        fetcher.check()?;
        let at = step(2);
        let bytes = fetcher
            .get_with_timeout(
                &format!("{}/{}.kml", self.site, url_key(key)),
                KML_TIMEOUT,
                &mut |b, t| {
                    progress(at(b, t));
                },
            )
            .map_err(|e| match e {
                TrackerError::Cancelled => e,
                other => TrackerError::Network(format!(
                    "the positions did not load ({failure}) and neither did the KML ({other})"
                )),
            })?;
        let mut tracks = crate::kml::parse_tracks(&bytes).map_err(|e| {
            TrackerError::Network(format!(
                "the positions did not load ({failure}) and the KML did not read ({e})"
            ))
        })?;
        Ok(event_of(
            event,
            &setup,
            |team| {
                // Each placemark serves the first team of its name.
                tracks
                    .iter()
                    .position(|t| t.name.trim() == team.name.trim())
                    .map(|k| tracks.remove(k).fixes)
                    .unwrap_or_default()
            },
            PositionsFrom::Fallback,
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn race_keys_come_from_every_address_form() {
        for input in [
            "fastnet2025",
            " fastnet2025 ",
            "https://yb.tl/fastnet2025",
            "http://yb.tl/fastnet2025/",
            "https://cf.yb.tl/fastnet2025#1",
            "https://cf.yb.tl/JSON/fastnet2025/RaceSetup",
            "https://cf.yb.tl/BIN/fastnet2025/AllPositions3",
            "app.yb.tl/?race=fastnet2025",
            "yb.tl/fastnet2025",
        ] {
            assert_eq!(race_key(input).expect(input), "fastnet2025", "{input}");
        }
    }

    #[test]
    fn mobile_codes_with_path_punctuation_round_trip() {
        for (key, url) in [
            ("stars&spokes2023", "https://yb.tl/stars&spokes2023"),
            ("n2e2026.admin", "https://yb.tl/n2e2026.admin"),
            ("ISORA2026_ 05CW3", "https://yb.tl/ISORA2026_%2005CW3"),
        ] {
            assert_eq!(race_key(url).unwrap(), key);
            assert_eq!(YellowBrick::default().resolve(key).unwrap().url, url);
        }
        assert_eq!(
            race_key("https://app.yb.tl/?race=stars%26spokes2023").unwrap(),
            "stars&spokes2023"
        );
    }

    /// A key goes into the request path as it is, so a key `race_key` would
    /// not produce is refused before any request.
    #[test]
    fn fetch_refuses_a_key_that_is_not_a_race_key() {
        let fetcher =
            Fetcher::new(TRACKER, Duration::from_secs(1), Default::default()).expect("a client");
        for key in ["", "../JSON/x", "a\nb", "x?y=1", &"k".repeat(65)] {
            let event = EventRef {
                tracker: Tracker::YellowBrick,
                key: key.to_owned(),
                url: String::new(),
            };
            let err = YellowBrick::default()
                .fetch(&event, &fetcher, &mut |_| {})
                .expect_err(key);
            assert!(
                matches!(err, TrackerError::NotAnEvent { .. }),
                "{key}: {err}"
            );
        }
    }

    #[test]
    fn resolve_gives_the_canonical_address() {
        let event = YellowBrick::default()
            .resolve("https://cf.yb.tl/BIN/fastnet2025/AllPositions3")
            .expect("resolves");
        assert_eq!(event.key, "fastnet2025");
        assert_eq!(event.url, "https://yb.tl/fastnet2025");
        assert!(is_html(b"\n\n<!DOCTYPE html>"));
        assert!(!is_html(&[0x02, 0x66]));
    }

    #[test]
    fn other_hosts_and_junk_are_refused() {
        for input in [
            "https://example.invalid/fastnet2025",
            "https://yb.tl/",
            "",
            "fast\nnet",
            "https://yb.tl/%2E%2E",
            "https://yb.tl/race%2Fother",
            "https://yb.tl.example.invalid/fastnet2025",
        ] {
            assert!(race_key(input).is_err(), "{input}");
        }
    }

    /// Hand-built: flags 0 (no optional fields), refTime 1000, one team
    /// (id 7) with an absolute newest moment and one delta 60 s older.
    #[test]
    fn a_hand_built_file_decodes_oldest_first() {
        let mut b = vec![0u8];
        b.extend_from_slice(&1000u32.to_be_bytes());
        b.extend_from_slice(&7u16.to_be_bytes());
        b.extend_from_slice(&2u16.to_be_bytes());
        b.extend_from_slice(&500u32.to_be_bytes()); // at 1500
        b.extend_from_slice(&5_000_000i32.to_be_bytes()); // 50N
        b.extend_from_slice(&(-500_000i32).to_be_bytes()); // 5W
        b.extend_from_slice(&(0x8000u16 | 60).to_be_bytes()); // 60 s older
        b.extend_from_slice(&100i16.to_be_bytes());
        b.extend_from_slice(&(-200i16).to_be_bytes());
        let all = decode_all_positions(&b).expect("decodes");
        assert_eq!(all.ref_time, 1000);
        let m = &all.teams[0].moments;
        assert_eq!(all.teams[0].id, 7);
        assert_eq!((m[0].at, m[0].lat, m[0].lon), (1440, 50.001, -5.002));
        assert_eq!((m[1].at, m[1].lat, m[1].lon), (1500, 50.0, -5.0));
    }

    /// With every flag set the record lengths grow as Appendix B says:
    /// absolute 12 + 2 + 4 + 1 + 4, delta 6 + 2 + 2 + 1 + 2 bytes.
    #[test]
    fn every_optional_field_is_consumed() {
        let mut b = vec![0x0F];
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&1u16.to_be_bytes());
        b.extend_from_slice(&2u16.to_be_bytes());
        b.extend_from_slice(&10u32.to_be_bytes());
        b.extend_from_slice(&0i32.to_be_bytes());
        b.extend_from_slice(&0i32.to_be_bytes());
        b.extend_from_slice(&12i16.to_be_bytes()); // alt
        b.extend_from_slice(&9000i32.to_be_bytes()); // dtf
        b.push(2); // lap
        b.extend_from_slice(&55i32.to_be_bytes()); // pc
        b.extend_from_slice(&(0x8000u16 | 5).to_be_bytes());
        b.extend_from_slice(&0i16.to_be_bytes());
        b.extend_from_slice(&0i16.to_be_bytes());
        b.extend_from_slice(&13i16.to_be_bytes());
        b.extend_from_slice(&100i16.to_be_bytes()); // dDtf
        b.push(2);
        b.extend_from_slice(&54i16.to_be_bytes());
        let all = decode_all_positions(&b).expect("decodes");
        let m = &all.teams[0].moments;
        assert_eq!(m[1].dtf, Some(9000));
        assert_eq!(m[0].dtf, Some(9100));
        assert_eq!(m[0].at, 5);
        assert_eq!((m[1].alt, m[1].lap, m[1].pc), (Some(12), Some(2), Some(55)));
    }

    #[test]
    fn truncation_and_a_leading_delta_name_the_byte() {
        let mut b = vec![0u8, 0, 0, 0, 0, 0, 1, 0, 1];
        b.extend_from_slice(&[0x80, 0, 0, 0, 0, 0]);
        let err = decode_all_positions(&b).expect_err("a delta first");
        assert!(err.to_string().contains("byte 9"), "{err}");
        let err =
            decode_all_positions(&[0u8, 0, 0, 0, 0, 0, 1, 0, 1, 0, 0]).expect_err("truncated");
        assert!(err.to_string().contains("byte 9"), "{err}");
        assert!(
            decode_all_positions(&[0xF0, 0, 0, 0, 0]).is_err(),
            "unknown flags"
        );
        assert!(decode_all_positions(&[]).is_err());
    }

    #[test]
    fn an_impossible_latitude_is_refused() {
        let mut b = vec![0u8, 0, 0, 0, 0, 0, 1, 0, 1];
        b.extend_from_slice(&0u32.to_be_bytes());
        b.extend_from_slice(&9_100_000i32.to_be_bytes());
        b.extend_from_slice(&0i32.to_be_bytes());
        assert!(decode_all_positions(&b).is_err());
    }

    /// ISO-8859-1, not UTF-8: 0xE7 is "ç".
    #[test]
    fn race_setup_is_latin1() {
        let bytes = b"{\"title\":\"T\",\"teams\":[{\"id\":3,\"name\":\"Fran\xe7ois\"}]}";
        let setup = parse_race_setup(bytes).expect("parses");
        assert_eq!(setup.teams[0].name, "François");
        let err = parse_race_setup(b"{\n\"title\": }").expect_err("bad JSON");
        assert!(err.to_string().contains("line 2"), "{err}");
    }
}
