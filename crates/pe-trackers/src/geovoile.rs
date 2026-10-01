//! Geovoile (spec.md 7.2, plan.md Appendix A).
//!
//! A Geovoile viewer (the `tracker/` generation, about 2016 on) reads its
//! data as `.hwx` resources: a 24-bit xorshift keystream XORed over an LZSS
//! stream. The four seeds differ per site and are carried in the viewer HTML,
//! so they are always parsed from it and never hard-coded (CLAUDE.md
//! "Environment gotchas").
//!
//! Every decode is checked for plausibility — it must parse, and its first
//! fix must be a sensible time and place — so a format change is refused as
//! "unsupported Geovoile version" rather than imported as garbage.

use std::collections::BTreeMap;

use base64::Engine;
use serde::Deserialize;

use pe_core::track::{Fix, Tracker};

use crate::error::{Result, TrackerError};
use crate::event::{EventRef, PositionsFrom, Progress, TrackerBoat, TrackerClient, TrackerEvent};
use crate::http::Fetcher;

const TRACKER: &str = "Geovoile";

/// The scheme every Geovoile request uses. Kept apart from the host so a
/// URL in this file always names its host literally for
/// `tools/check-offline.sh`, and the hosts built from a pasted address are
/// checked by [`crate::net::is_geovoile_host`] instead.
const HTTPS: &str = "https://";

/// The four 24-bit keystream seeds, in `x, y, z, w` order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Seeds(pub [u32; 4]);

fn unsupported(why: impl Into<String>) -> TrackerError {
    TrackerError::Unsupported {
        tracker: TRACKER,
        why: why.into(),
    }
}

/// Reads the seeds out of the viewer HTML.
///
/// The HTML carries a `data:image/png;base64,…` source whose value continues
/// past the image in `/C/`-separated base64 script segments. The first
/// assigns four constants to window globals (`window._0xA0A0F0=0xFF8040;…`);
/// the second is the keystream function, whose first four `var` statements
/// initialise `x, y, z, w` from those globals (or, on older sites, from hex
/// literals). The order therefore comes from the function, not from the
/// assignment order, which is shuffled.
///
/// # Errors
/// [`TrackerError::Unsupported`] if the segments are absent or do not have
/// that shape.
pub fn seeds_from_html(html: &str) -> Result<Seeds> {
    // The segments may be spread over several images (Route du Rhum 2018:
    // the globals in one, the keystream in the next), and image data can
    // hold `/C/` by chance; so all of them are read in page order.
    let segments: Vec<String> = script_segments(html).into_iter().flatten().collect();
    if segments.is_empty() {
        return Err(unsupported(
            "the viewer page carries no hwx seeds; only the tracker/ generation (about 2016 on) is supported",
        ));
    }
    seeds_from_segments(&segments)
}

fn seeds_from_segments(segments: &[String]) -> Result<Seeds> {
    let mut globals: BTreeMap<String, u32> = BTreeMap::new();
    let mut function: Option<&str> = None;
    for segment in segments {
        if segment.contains("function") && segment.contains("0xFFFFFF") {
            function = Some(segment);
            break;
        }
        for statement in segment.split(';') {
            if let Some((name, value)) = statement
                .trim()
                .strip_prefix("window.")
                .and_then(|s| s.split_once('='))
                && let Some(value) = parse_hex(value)
            {
                globals.insert(name.trim().to_owned(), value);
            }
        }
    }
    let function = function.ok_or_else(|| unsupported("the viewer page has no hwx keystream"))?;
    // The body's first four `var a=b;` statements are the state.
    let body = function
        .split_once('{')
        .map(|(_, body)| body)
        .ok_or_else(|| unsupported("the hwx keystream has no body"))?;
    let mut seeds = Vec::with_capacity(4);
    for statement in body.split(';') {
        let Some(assignment) = statement.trim().strip_prefix("var ") else {
            break;
        };
        let Some((_, value)) = assignment.split_once('=') else {
            break;
        };
        let value = value.trim();
        let seed = parse_hex(value)
            .or_else(|| globals.get(value).copied())
            .ok_or_else(|| unsupported(format!("hwx seed {value:?} is not defined in the page")))?;
        seeds.push(seed);
        if seeds.len() == 4 {
            break;
        }
    }
    match seeds.as_slice() {
        [x, y, z, w] if seeds.iter().all(|s| *s <= 0xFF_FFFF) => Ok(Seeds([*x, *y, *z, *w])),
        _ => Err(unsupported(
            "the hwx keystream does not start with four 24-bit seeds",
        )),
    }
}

fn parse_hex(text: &str) -> Option<u32> {
    let digits = text
        .trim()
        .strip_prefix("0x")
        .or_else(|| text.trim().strip_prefix("0X"))?;
    u32::from_str_radix(digits, 16).ok()
}

/// The decoded `/C/` script segments of each `data:image/png` source that
/// has any.
fn script_segments(html: &str) -> Vec<Vec<String>> {
    const MARK: &str = "data:image/png;base64,";
    let mut out = Vec::new();
    let mut search = html;
    while let Some(start) = search.find(MARK) {
        let value = &search[start + MARK.len()..];
        let end = value
            .find(['"', '\'', ')', ' ', '\n'])
            .unwrap_or(value.len());
        let value = &value[..end];
        let mut parts = value.split("/C/");
        let _image = parts.next();
        let segments: Vec<String> = parts
            .filter_map(|p| base64::engine::general_purpose::STANDARD.decode(p).ok())
            .filter_map(|bytes| String::from_utf8(bytes).ok())
            .collect();
        if !segments.is_empty() {
            out.push(segments);
        }
        search = &search[start + MARK.len()..];
    }
    out
}

/// The viewer parameters an import needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Viewer {
    /// Page title.
    pub title: String,
    /// `rooturl`, e.g. `/2025/`.
    pub root_url: String,
    /// `resourcesurl`: a static resource host, or empty.
    pub resources_url: String,
    /// `versionsurl`: where the versions file is, when not beside the
    /// resources; usually empty.
    pub versions_url: String,
    /// Number of legs.
    pub legs: u32,
    /// The leg this page shows.
    pub leg: u32,
    /// The hwx seeds.
    pub seeds: Seeds,
}

/// The value of `key :'…'` or `key :"…"` or `key :123` in the viewer's
/// inline parameter object.
fn param<'a>(html: &'a str, key: &str) -> Option<&'a str> {
    let mut rest = html;
    while let Some(at) = rest.find(key) {
        let after = &rest[at + key.len()..];
        let boundary_ok = rest[..at]
            .chars()
            .next_back()
            .is_none_or(|c| !c.is_ascii_alphanumeric() && c != '_');
        let trimmed = after.trim_start();
        if boundary_ok && let Some(value) = trimmed.strip_prefix(':') {
            let value = value.trim_start();
            if let Some(q) = value.chars().next().filter(|c| *c == '\'' || *c == '"') {
                let inner = &value[1..];
                return inner.find(q).map(|end| &inner[..end]);
            }
            let end = value
                .find(|c: char| !c.is_ascii_alphanumeric())
                .unwrap_or(value.len());
            return Some(&value[..end]);
        }
        rest = after;
    }
    None
}

/// Parses the viewer page.
///
/// # Errors
/// [`TrackerError::Unsupported`] for a page without the `tracker/`
/// generation's parameters or seeds.
pub fn parse_viewer(html: &str) -> Result<Viewer> {
    let legacy = |why: &str| TrackerError::Legacy {
        tracker: TRACKER,
        why: why.to_owned(),
    };
    let Some(root_url) = param(html, "rooturl") else {
        let lower = html.to_ascii_lowercase();
        return Err(if lower.contains(".hwz") || lower.contains(".swf") {
            legacy(
                "it is a Flash tracker (.hwz); only the tracker/ generation (about 2016 on) is read",
            )
        } else if lower.contains("<html") {
            legacy(
                "the page is not a tracker/ viewer; Geovoile's 2012–2015 trackers are not read, only the tracker/ generation (about 2016 on)",
            )
        } else {
            unsupported("the viewer address answered with something that is not a web page")
        });
    };
    let root_url = root_url.to_owned();
    let number = |key: &str| param(html, key).and_then(|v| v.parse::<u32>().ok());
    let title = html
        .find("<title>")
        .and_then(|at| {
            let rest = &html[at + 7..];
            rest.find("</title>")
                .map(|end| rest[..end].trim().to_owned())
        })
        .unwrap_or_default();
    // `nblegs` is clamped rather than trusted outright, and `numleg` (the
    // leg the page says it shows) must be one this race actually has;
    // either off is a page this build does not understand, not a wrong leg
    // silently opened.
    let legs = number("nblegs").unwrap_or(1).clamp(1, 99);
    let leg = number("numleg").unwrap_or(1);
    if !(1..=legs).contains(&leg) {
        return Err(unsupported(format!(
            "the viewer names leg {leg} of {legs}, which is not a leg this race has"
        )));
    }
    Ok(Viewer {
        title,
        root_url,
        resources_url: param(html, "resourcesurl").unwrap_or_default().to_owned(),
        versions_url: param(html, "versionsurl").unwrap_or_default().to_owned(),
        legs,
        leg,
        seeds: seeds_from_html(html)?,
    })
}

impl Viewer {
    /// Where a resource of `kind` (`config`, `tracks`, `reports`) at
    /// `version` is, relative to the site origin — the viewer's own rule.
    pub fn resource_path(&self, kind: &str, version: u64) -> String {
        let is_static = !self.resources_url.is_empty();
        // The viewer's own rule (`_getRessourceUrl` in its viewer.js).
        let leg = if self.legs > 1 {
            format!("leg{}{}", self.leg, if is_static { "_" } else { "/" })
        } else {
            String::new()
        };
        if is_static {
            format!("{}{leg}tracker_{kind}.hwx?v={version}", self.resources_url)
        } else {
            format!("{}tracker/resources/{leg}{kind}/v{version}", self.root_url)
        }
    }
}

/// Parses the versions resource, a JS object literal with bare keys:
/// `{config:20220308182522,tracks:20220308182529,…}`.
///
/// # Errors
/// [`TrackerError::Decode`] naming the entry that does not parse.
pub fn parse_versions(text: &str) -> Result<BTreeMap<String, u64>> {
    let body = text
        .trim()
        .strip_prefix('{')
        .and_then(|t| t.strip_suffix('}'))
        .ok_or_else(|| TrackerError::Decode {
            what: "Geovoile versions",
            at: "column 1".to_owned(),
            why: "not an object literal".to_owned(),
        })?;
    let mut out = BTreeMap::new();
    for (i, entry) in body.split(',').enumerate() {
        if entry.trim().is_empty() {
            continue;
        }
        let parsed = entry.split_once(':').and_then(|(k, v)| {
            let k = k.trim().trim_matches(|c| c == '"' || c == '\'');
            let v = v.trim().trim_matches(|c| c == '"' || c == '\'');
            v.parse::<u64>().ok().map(|v| (k.to_owned(), v))
        });
        let (k, v) = parsed.ok_or_else(|| TrackerError::Decode {
            what: "Geovoile versions",
            at: format!("entry {}", i + 1),
            why: format!("{entry:?} is not key:number"),
        })?;
        out.insert(k, v);
    }
    Ok(out)
}

/// The xorshift keystream (Appendix A). All arithmetic is masked to 24 bits.
struct Keystream {
    x: u32,
    y: u32,
    z: u32,
    w: u32,
}

impl Keystream {
    fn new(seeds: Seeds, skip: u8) -> Self {
        let [x, y, z, w] = seeds.0;
        let mut k = Self { x, y, z, w };
        for _ in 0..skip {
            k.step();
        }
        k
    }

    fn step(&mut self) {
        const M: u32 = 0xFF_FFFF;
        let mut t = self.x;
        t ^= (t << 11) & M;
        t ^= (t >> 8) & M;
        self.x = self.y;
        self.y = self.z;
        self.z = self.w;
        self.w ^= (self.w >> 19) & M;
        self.w ^= t;
    }

    fn dec(&mut self, b: u8) -> u8 {
        let r = b ^ (self.x & 0xFF) as u8;
        self.step();
        r
    }
}

fn hwx_error(at: usize, why: impl Into<String>) -> TrackerError {
    TrackerError::Decode {
        what: "Geovoile hwx",
        at: format!("byte {at}"),
        why: why.into(),
    }
}

/// Decodes an `.hwx` resource (Appendix A): a keystream skip count, a 24-bit
/// output length, then LZSS groups of eight items under a flag byte. The
/// flag bytes are *not* keystream-decoded; everything else is.
///
/// # Errors
/// [`TrackerError::Decode`] naming the byte where the input ran out or a
/// back-reference pointed before the start of the output.
pub fn decode_hwx(buf: &[u8], seeds: Seeds) -> Result<Vec<u8>> {
    let byte = |i: usize| {
        buf.get(i)
            .copied()
            .ok_or_else(|| hwx_error(i, "the resource ends early"))
    };
    let mut k = Keystream::new(seeds, byte(0)?);
    let out_len = (usize::from(k.dec(byte(1)?)) << 16)
        | (usize::from(k.dec(byte(2)?)) << 8)
        | usize::from(k.dec(byte(3)?));
    let mut out: Vec<u8> = Vec::with_capacity(out_len);
    let mut i = 4;
    while out.len() < out_len {
        let flags = byte(i)? ^ (i & 0xFF) as u8 ^ 0xA3;
        i += 1;
        for bit in (0..8).rev() {
            if out.len() >= out_len {
                break;
            }
            if flags & (1 << bit) == 0 {
                out.push(k.dec(byte(i)?));
                i += 1;
            } else {
                let b = k.dec(byte(i)?);
                let len = usize::from(b >> 4) + 3;
                let offset = ((usize::from(b & 0xF) << 8) | usize::from(k.dec(byte(i + 1)?))) + 1;
                if offset > out.len() {
                    return Err(hwx_error(
                        i,
                        format!("a back-reference of {offset} bytes points before the start"),
                    ));
                }
                // Byte by byte: a copy may overlap what it is writing.
                for _ in 0..len {
                    out.push(out[out.len() - offset]);
                }
                i += 2;
            }
        }
    }
    out.truncate(out_len);
    Ok(out)
}

/// Decodes an hwx resource and checks it is UTF-8 text that starts the way
/// its kind should (XML for `config`, a JSON object otherwise). Wrong seeds
/// decode to noise, which fails here.
///
/// # Errors
/// [`TrackerError::Unsupported`] if the decoded bytes are not such text.
pub fn decode_text(buf: &[u8], seeds: Seeds, xml: bool) -> Result<String> {
    let bytes = decode_hwx(buf, seeds)
        .map_err(|e| unsupported(format!("the hwx resource did not decode ({e})")))?;
    let text = String::from_utf8(bytes).map_err(|_| {
        unsupported("the hwx resource did not decode to text; the seeds may be wrong")
    })?;
    let head = text.trim_start_matches('\u{feff}').trim_start();
    let ok = if xml {
        head.starts_with('<')
    } else {
        head.starts_with('{')
    };
    if ok {
        Ok(text)
    } else {
        Err(unsupported(
            "the hwx resource decoded to something unexpected",
        ))
    }
}

/// One boat in the config.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Boat {
    /// The id tracks name it by.
    pub id: u32,
    /// Boat name.
    pub name: String,
    /// Sail number, when given.
    pub sail: Option<String>,
    /// Track colour, `#rrggbb`, when given.
    pub colour: Option<String>,
    /// The id of the class (`boatclass`) it sails in.
    pub class: Option<u32>,
}

/// A class of boats (`boatclass`): Ultim, IMOCA, Class40…
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BoatClass {
    /// Its id.
    pub id: u32,
    /// Its name.
    pub name: String,
    /// The run (start) it sails, when given.
    pub run: Option<u32>,
}

/// A run: one start of the leg, which classes may share.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Run {
    /// Its id.
    pub id: u32,
    /// The start time, as written, when given.
    pub start: Option<String>,
}

/// The leg the config describes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Leg {
    /// Which leg, from 1.
    pub num: u32,
    /// How many legs the race has.
    pub total: u32,
}

/// The parts of the config an import uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Race name.
    pub name: String,
    /// The `date` attribute (race start), as written.
    pub date: Option<String>,
    /// The leg, when the config names one.
    pub leg: Option<Leg>,
    /// The classes, in config order.
    pub classes: Vec<BoatClass>,
    /// The runs (starts), in config order.
    pub runs: Vec<Run>,
    /// Every boat, in config order.
    pub boats: Vec<Boat>,
}

/// Parses the config XML.
///
/// # Errors
/// [`TrackerError::Decode`] with the byte position of malformed XML.
pub fn parse_config(xml: &str) -> Result<Config> {
    use quick_xml::events::{BytesStart, Event};
    let mut reader = quick_xml::Reader::from_str(xml);
    let attrs = |e: &BytesStart<'_>| -> BTreeMap<String, String> {
        e.attributes()
            .flatten()
            .filter_map(|a| {
                let key = a.key.into_inner().to_owned();
                a.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                    .ok()
                    .map(|v| (key, v.into_owned()))
            })
            .collect()
    };
    let number =
        |a: &BTreeMap<String, String>, k: &str| a.get(k).and_then(|v| v.trim().parse().ok());
    let non_empty = |a: &BTreeMap<String, String>, k: &str| {
        a.get(k)
            .map(|v| v.trim().to_owned())
            .filter(|v| !v.is_empty())
    };
    let mut config = Config {
        name: String::new(),
        date: None,
        leg: None,
        classes: Vec::new(),
        runs: Vec::new(),
        boats: Vec::new(),
    };
    // The class and the run being read: boats sit inside their class, and a
    // run's start inside the run.
    let mut class: Option<u32> = None;
    let mut run: Option<usize> = None;
    loop {
        let event = reader.read_event().map_err(|e| TrackerError::Decode {
            what: "Geovoile config",
            at: format!("byte {}", reader.buffer_position()),
            why: e.to_string(),
        })?;
        let (e, open) = match &event {
            Event::Start(e) => (e, true),
            Event::Empty(e) => (e, false),
            Event::End(e) => {
                match e.name().into_inner() {
                    "boatclass" => class = None,
                    "run" => run = None,
                    _ => {}
                }
                continue;
            }
            Event::Eof => break,
            _ => continue,
        };
        let a = attrs(e);
        match e.name().into_inner() {
            "config" => {
                config.name = a
                    .get("name")
                    .map(|v| v.trim().to_owned())
                    .unwrap_or_default();
                config.date = non_empty(&a, "date");
            }
            "leg" if config.leg.is_none() => {
                if let (Some(num), Some(total)) = (number(&a, "num"), number(&a, "total")) {
                    config.leg = Some(Leg { num, total });
                }
            }
            "run" => {
                if let Some(id) = number(&a, "id") {
                    config.runs.push(Run { id, start: None });
                    run = open.then(|| config.runs.len() - 1);
                }
            }
            "start" => {
                if let Some(k) = run
                    && let Some(r) = config.runs.get_mut(k)
                {
                    r.start = non_empty(&a, "date");
                }
            }
            "boatclass" => {
                if let Some(id) = number(&a, "id") {
                    config.classes.push(BoatClass {
                        id,
                        name: a
                            .get("name")
                            .map(|v| v.trim().to_owned())
                            .unwrap_or_default(),
                        run: number(&a, "runid"),
                    });
                    class = open.then_some(id);
                }
            }
            "boat" => {
                let Some(id) = number(&a, "id") else {
                    continue;
                };
                config.boats.push(Boat {
                    id,
                    name: a
                        .get("name")
                        .map(|v| v.trim().to_owned())
                        .unwrap_or_default(),
                    sail: non_empty(&a, "sail"),
                    colour: non_empty(&a, "trackcolor")
                        .filter(|c| c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
                        .map(|c| format!("#{}", c.to_ascii_lowercase())),
                    class,
                });
            }
            _ => {}
        }
    }
    Ok(config)
}

#[derive(Deserialize)]
struct RawTracks {
    tracks: Vec<RawTrack>,
}

#[derive(Deserialize)]
struct RawTrack {
    id: u32,
    /// A boat's positions are a list of triples. Some sites add a track that
    /// is not a boat (id 0 on the 24 Heures Ultim: a flat list of numbers),
    /// so the shape is checked per track rather than by the type.
    #[serde(default)]
    loc: serde_json::Value,
}

/// One boat's positions, oldest first.
#[derive(Debug, Clone, PartialEq)]
pub struct Track {
    /// Config boat id.
    pub id: u32,
    /// Fixes, oldest first.
    pub fixes: Vec<Fix>,
}

/// Parses the tracks resource: per boat, `loc` holds `[t, lat·1e5,
/// lon·1e5]` and then `[dt, dlat, dlon]` deltas.
///
/// # Errors
/// [`TrackerError::Decode`] for malformed JSON or a short entry;
/// [`TrackerError::Unsupported`] if the first fix is not a plausible time
/// (2000–2100) and position, which is how a changed format shows itself.
pub fn parse_tracks(json: &str) -> Result<Vec<Track>> {
    let raw: RawTracks = serde_json::from_str(json).map_err(|e| TrackerError::Decode {
        what: "Geovoile tracks",
        at: format!("line {} column {}", e.line(), e.column()),
        why: e.to_string(),
    })?;
    let mut tracks = Vec::with_capacity(raw.tracks.len());
    for track in raw.tracks {
        let Some(entries) = track.loc.as_array() else {
            continue;
        };
        if entries.first().is_some_and(|e| !e.is_array()) {
            // Not a boat's positions.
            continue;
        }
        let mut fixes = Vec::with_capacity(entries.len());
        let (mut t, mut lat, mut lon) = (0i64, 0i64, 0i64);
        for (i, entry) in entries.iter().enumerate() {
            let values: Vec<i64> = entry
                .as_array()
                .map(|a| a.iter().map_while(serde_json::Value::as_i64).collect())
                .unwrap_or_default();
            // Extra values after the third, if a viewer ever adds some, are
            // not positions and are ignored.
            let &[a, b, c, ..] = values.as_slice() else {
                return Err(TrackerError::Decode {
                    what: "Geovoile tracks",
                    at: format!("boat {} entry {i}", track.id),
                    why: format!("{entry} is not three whole numbers"),
                });
            };
            (t, lat, lon) = if i == 0 {
                (a, b, c)
            } else {
                match (t.checked_add(a), lat.checked_add(b), lon.checked_add(c)) {
                    (Some(t), Some(lat), Some(lon)) => (t, lat, lon),
                    _ => {
                        return Err(unsupported(format!(
                            "boat {}'s entry {i} overflows its running position",
                            track.id
                        )));
                    }
                }
            };
            // Every fix, not only the first: a changed encoding can start
            // plausibly and drift off the globe.
            let fix = fix(t, lat, lon);
            let plausible_time = (946_684_800..4_102_444_800).contains(&fix.t);
            let plausible_place = (-90.0..=90.0).contains(&fix.lat) && fix.lon.is_finite();
            if !plausible_time || !plausible_place {
                return Err(unsupported(format!(
                    "boat {}'s fix {i} ({}, {}, {}) is not a plausible time and place",
                    track.id, fix.t, fix.lat, fix.lon
                )));
            }
            fixes.push(fix);
        }
        tracks.push(Track {
            id: track.id,
            fixes,
        });
    }
    Ok(tracks)
}

fn fix(t: i64, lat: i64, lon: i64) -> Fix {
    let lon = lon as f64 / 1e5;
    Fix {
        tws: None,
        twd_from: None,
        t,
        lat: lat as f64 / 1e5,
        lon: crate::wrap_lon(lon),
        cog: None,
        sog: None,
    }
}

// ------------------------------------------------------------------ reports

/// One boat's line in one report: the tracker's official figures at the
/// report's time.
#[derive(Debug, Clone, PartialEq)]
pub struct ReportLine {
    /// The config boat id.
    pub boat: u32,
    /// The report's time, UTC epoch seconds.
    pub t: i64,
    /// The race status code (`RAC`, `ARV`, `RET`, `DNF`…), when given.
    pub status: Option<String>,
    /// Official heading, degrees; 0 means none.
    pub heading: f64,
    /// Official speed, knots; 0 means none.
    pub speed: f64,
}

/// The parts of the reports resource an import uses.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Reports {
    /// Every boat's line of every report, oldest report first.
    pub lines: Vec<ReportLine>,
    /// Finish times by boat id.
    pub arrivals: BTreeMap<u32, i64>,
    /// When a boat was hidden (withdrawn from the map), by boat id.
    pub hidden: BTreeMap<u32, i64>,
}

fn iso(text: &str) -> Option<i64> {
    pe_tracks::time::parse_time(text, &pe_tracks::time::TimeFormat::Iso8601).ok()
}

/// A boat id as the reports write it: a string or a number.
fn boat_id(value: &serde_json::Value) -> Option<u32> {
    match value {
        serde_json::Value::String(s) => s.trim().parse().ok(),
        other => other.as_u64().and_then(|n| u32::try_from(n).ok()),
    }
}

/// Parses the reports resource: `{"reports": {"columns": [...], "history":
/// [{"date", "lines": [[...], ...]}], "arrivals": [[id, date, ...]],
/// "hidden": [[id, date]]}}`. Each line's values follow `columns`, whose
/// order differs between editions, so fields are found by name.
///
/// # Errors
/// [`TrackerError::Decode`] for malformed JSON or a history without the
/// `boat`, `heading` and `speed` columns.
pub fn parse_reports(json: &str) -> Result<Reports> {
    let value: serde_json::Value =
        serde_json::from_str(json).map_err(|e| TrackerError::Decode {
            what: "Geovoile reports",
            at: format!("line {} column {}", e.line(), e.column()),
            why: e.to_string(),
        })?;
    let reports = &value["reports"];
    let bad = |why: &str| TrackerError::Decode {
        what: "Geovoile reports",
        at: "reports".to_owned(),
        why: why.to_owned(),
    };
    let columns: Vec<&str> = reports["columns"]
        .as_array()
        .ok_or_else(|| bad("no columns"))?
        .iter()
        .map(|c| c.as_str().unwrap_or(""))
        .collect();
    let column = |name: &str| columns.iter().position(|c| *c == name);
    let (Some(boat_col), Some(heading_col), Some(speed_col)) =
        (column("boat"), column("heading"), column("speed"))
    else {
        return Err(bad("the columns have no boat, heading and speed"));
    };
    let status_col = column("racestatus");
    let mut out = Reports::default();
    for report in reports["history"].as_array().map_or(&[][..], Vec::as_slice) {
        let Some(t) = report["date"].as_str().and_then(iso) else {
            continue;
        };
        for line in report["lines"].as_array().map_or(&[][..], Vec::as_slice) {
            let Some(values) = line.as_array() else {
                continue;
            };
            let Some(boat) = values.get(boat_col).and_then(boat_id) else {
                continue;
            };
            let number = |k: usize| values.get(k).and_then(serde_json::Value::as_f64);
            out.lines.push(ReportLine {
                boat,
                t,
                status: status_col
                    .and_then(|k| values.get(k))
                    .and_then(serde_json::Value::as_str)
                    .map(str::to_owned),
                heading: number(heading_col).unwrap_or(0.0),
                speed: number(speed_col).unwrap_or(0.0),
            });
        }
    }
    out.lines.sort_by_key(|l| l.t);
    let dated = |key: &str| -> BTreeMap<u32, i64> {
        reports[key]
            .as_array()
            .map_or(&[][..], Vec::as_slice)
            .iter()
            .filter_map(|entry| {
                let boat = boat_id(entry.get(0)?)?;
                let t = iso(entry.get(1)?.as_str()?)?;
                Some((boat, t))
            })
            .collect()
    };
    out.arrivals = dated("arrivals");
    out.hidden = dated("hidden");
    Ok(out)
}

/// How far from a fix a report may be and still describe it. Reports are
/// at round times and the fixes near them (24 Heures Ultim 2025: most
/// within a minute).
pub const REPORT_MATCH_S: i64 = 60;

/// Gives each fix the official heading and speed of the report nearest in
/// time (within [`REPORT_MATCH_S`]), each only when non-zero and sensible
/// (spec.md 7.2); a fix no report is near keeps none, to be derived.
pub fn apply_reports(fixes: &mut [Fix], lines: &[&ReportLine]) {
    // For each fix, the nearest report so far and its distance.
    let mut best: Vec<Option<(i64, usize)>> = vec![None; fixes.len()];
    for (k, line) in lines.iter().enumerate() {
        let at = fixes.partition_point(|f| f.t < line.t);
        let nearest = [at.checked_sub(1), Some(at)]
            .into_iter()
            .flatten()
            .filter(|&i| i < fixes.len())
            .min_by_key(|&i| (fixes[i].t - line.t).abs());
        let Some(i) = nearest else {
            continue;
        };
        let gap = (fixes[i].t - line.t).abs();
        if gap <= REPORT_MATCH_S && best[i].is_none_or(|(g, _)| gap < g) {
            best[i] = Some((gap, k));
        }
    }
    for (fix, best) in fixes.iter_mut().zip(best) {
        let Some((_, k)) = best else {
            continue;
        };
        let line = lines[k];
        if line.heading != 0.0 && (0.0..=360.0).contains(&line.heading) {
            fix.cog = Some(line.heading % 360.0);
        }
        if line.speed > 0.0 && line.speed < 100.0 {
            fix.sog = Some(line.speed);
        }
    }
}

/// A report's status code as the dialog's status (spec.md 7.2).
fn status(code: &str) -> String {
    match code {
        "RAC" | "STA" => "RACING".to_owned(),
        "ARV" => "FINISHED".to_owned(),
        "RET" | "ABD" => "RETIRED".to_owned(),
        other => other.to_owned(),
    }
}

// --------------------------------------------------------------------- site

/// A Geovoile event as a pasted address names it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Site {
    /// The host, a subdomain of `geovoile.com`.
    pub host: String,
    /// The race's root path, e.g. `/2024/`.
    pub root: String,
    /// The leg asked for, when the address names one.
    pub leg: Option<u32>,
}

/// Whether one path segment is safe to put in a request as it is.
fn safe_segment(segment: &str) -> bool {
    !segment.is_empty()
        && segment != "."
        && segment != ".."
        && segment
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

/// The event a pasted viewer address names: `https://<sub>.geovoile.com/
/// <root>/tracker/` or `/viewer/`, with an optional `?leg=<n>`; the scheme
/// may be left out, and so may `tracker/`.
///
/// # Errors
/// [`TrackerError::NotAnEvent`] for another host (compared exactly, so
/// look-alikes and addresses with a user name are refused) or no root;
/// [`TrackerError::Legacy`] for a Flash tracker's address.
pub fn site(input: &str) -> Result<Site> {
    let not_an_event = || TrackerError::NotAnEvent {
        tracker: TRACKER,
        input: input.to_owned(),
    };
    let text = input.trim();
    let text = if text.contains("://") {
        text.to_owned()
    } else {
        format!("{HTTPS}{text}")
    };
    let url = reqwest::Url::parse(&text).map_err(|_| not_an_event())?;
    let host = url
        .host_str()
        .ok_or_else(not_an_event)?
        .to_ascii_lowercase();
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !crate::net::is_geovoile_host(&host)
    {
        return Err(not_an_event());
    }
    let segments: Vec<&str> = url
        .path_segments()
        .map(|s| s.filter(|s| !s.is_empty()).collect())
        .unwrap_or_default();
    if segments.iter().any(|s| {
        let s = s.to_ascii_lowercase();
        s.ends_with(".hwz") || s.ends_with(".swf")
    }) {
        return Err(TrackerError::Legacy {
            tracker: TRACKER,
            why:
                "it is a Flash tracker (.hwz); only the tracker/ generation (about 2016 on) is read"
                    .to_owned(),
        });
    }
    let viewer = segments
        .iter()
        .position(|s| s.eq_ignore_ascii_case("tracker") || s.eq_ignore_ascii_case("viewer"));
    let root: &[&str] = match viewer {
        Some(k) => &segments[..k],
        // A page name at the end (`index.html`) is not part of the root.
        None => match segments.split_last() {
            Some((last, rest)) if last.contains('.') => rest,
            _ => &segments,
        },
    };
    if root.is_empty() || !root.iter().all(|s| safe_segment(s)) {
        return Err(not_an_event());
    }
    let mut leg = None;
    for (key, value) in url.query_pairs() {
        if key == "leg" {
            leg = Some(
                value
                    .parse::<u32>()
                    .ok()
                    .filter(|n| (1..=99).contains(n))
                    .ok_or_else(not_an_event)?,
            );
        }
    }
    Ok(Site {
        host,
        root: format!("/{}/", root.join("/")),
        leg,
    })
}

impl Site {
    fn leg_query(&self) -> String {
        self.leg.map(|n| format!("?leg={n}")).unwrap_or_default()
    }

    /// The session key: host, root and leg.
    pub fn key(&self) -> String {
        format!("{}{}{}", self.host, self.root, self.leg_query())
    }

    /// The viewer page, which is also the event's canonical address.
    pub fn url(&self) -> String {
        format!(
            "{HTTPS}{}{}tracker/{}",
            self.host,
            self.root,
            self.leg_query()
        )
    }
}

/// Resolves `path` (absolute, root-relative or relative) against the viewer
/// page and checks the result is HTTPS on a Geovoile host (invariant 4):
/// `resourcesurl` and `versionsurl` come from the page and are never
/// requested elsewhere.
///
/// # Errors
/// [`TrackerError::Unsupported`] for any other address.
pub fn resource_url(page: &reqwest::Url, path: &str) -> Result<reqwest::Url> {
    let url = page.join(path).map_err(|_| {
        unsupported(format!(
            "the viewer names a resource at {path:?}, which is not an address"
        ))
    })?;
    let host = url.host_str().unwrap_or_default();
    if url.scheme() != "https"
        || !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || !crate::net::is_geovoile_host(host)
    {
        return Err(unsupported(format!(
            "the viewer's resources are at {path:?}, which is not a Geovoile address"
        )));
    }
    Ok(url)
}

// ------------------------------------------------------------------- client

/// The Geovoile client (spec.md 7.2).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Geovoile {
    /// Where requests go instead of the Geovoile host: the fixture tests
    /// serve recorded responses from a local server. The URLs are still
    /// built and checked as for the real host.
    base: Option<String>,
}

/// How many requests a download makes: the page, the versions, the config,
/// the tracks and the reports.
const STEPS: u32 = 5;

impl Geovoile {
    /// A client that sends every request's path and query to `base`.
    pub fn at(base: &str) -> Self {
        Self {
            base: Some(base.trim_end_matches('/').to_owned()),
        }
    }

    fn request(&self, url: &reqwest::Url) -> String {
        match &self.base {
            Some(base) => {
                let query = url.query().map(|q| format!("?{q}")).unwrap_or_default();
                format!("{base}{}{query}", url.path())
            }
            None => url.to_string(),
        }
    }
}

/// The viewer's cache-busting version for a file the versions do not name:
/// the time rounded down to five seconds, as the viewer does.
fn now_version() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() / 5 * 5)
}

/// Builds the event from the config, the tracks and (when they loaded) the
/// reports, in the config's boat order.
pub fn event_of(
    event: EventRef,
    viewer: &Viewer,
    config: &Config,
    tracks: Vec<Track>,
    reports: Option<&Reports>,
) -> TrackerEvent {
    let mut by_id: BTreeMap<u32, Vec<Fix>> = tracks.into_iter().map(|t| (t.id, t.fixes)).collect();
    let mut lines_of: BTreeMap<u32, Vec<&ReportLine>> = BTreeMap::new();
    if let Some(reports) = reports {
        for line in &reports.lines {
            lines_of.entry(line.boat).or_default().push(line);
        }
    }
    let config_start = config.date.as_deref().and_then(iso);
    let mut boats: Vec<TrackerBoat> = config
        .boats
        .iter()
        .map(|boat| {
            let mut fixes = by_id.remove(&boat.id).unwrap_or_default();
            let lines = lines_of.remove(&boat.id).unwrap_or_default();
            apply_reports(&mut fixes, &lines);
            let class = boat
                .class
                .and_then(|id| config.classes.iter().find(|c| c.id == id));
            let run_start = class
                .and_then(|c| c.run)
                .and_then(|id| config.runs.iter().find(|r| r.id == id))
                .and_then(|r| r.start.as_deref())
                .and_then(iso);
            TrackerBoat {
                details: Default::default(),
                id: boat.id.to_string(),
                name: boat.name.clone(),
                sail: boat.sail.clone(),
                model: None,
                division: class.map(|c| c.name.clone()).filter(|n| !n.is_empty()),
                status: if reports.is_some_and(|r| r.arrivals.contains_key(&boat.id)) {
                    Some("FINISHED".into())
                } else {
                    lines.last().and_then(|l| l.status.as_deref()).map(status)
                },
                start: run_start.or(config_start),
                finish: reports.and_then(|r| {
                    r.arrivals
                        .get(&boat.id)
                        .or_else(|| r.hidden.get(&boat.id))
                        .copied()
                }),
                fixes,
            }
        })
        .collect();
    let first = boats
        .iter()
        .filter_map(|b| b.fixes.first())
        .map(|f| f.t)
        .min();
    let stop = boats
        .iter()
        .filter_map(|b| b.fixes.last())
        .map(|f| f.t)
        .max()
        .or_else(|| reports.and_then(|r| r.lines.last().map(|line| line.t)));
    for boat in &mut boats {
        boat.finish = boat.finish.or(stop);
    }
    let name = if config.name.is_empty() {
        viewer.title.clone()
    } else {
        config.name.clone()
    };
    let title = if viewer.legs > 1 {
        format!("{name} ({}/{})", viewer.leg, viewer.legs)
    } else {
        name
    };
    TrackerEvent {
        event,
        title,
        start: config_start.or(first),
        stop,
        boats,
        positions_from: PositionsFrom::Primary,
        leg: (viewer.legs > 1).then_some((viewer.leg, viewer.legs)),
    }
}

impl TrackerClient for Geovoile {
    fn tracker(&self) -> Tracker {
        Tracker::Geovoile
    }

    fn fetch_for_scrape(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        now: i64,
    ) -> Result<crate::library::completion::ScrapeFetch> {
        let metadata = self.fetch_inner(event, fetcher, &mut |_| {}, &mut |_| {}, true)?;
        crate::library::completion::after_check(metadata, now, || {
            self.fetch(event, fetcher, progress)
        })
    }

    fn resolve(&self, input: &str) -> Result<EventRef> {
        let site = site(input)?;
        Ok(EventRef {
            tracker: Tracker::Geovoile,
            key: site.key(),
            url: site.url(),
        })
    }

    /// The viewer page (for the parameters and seeds), the versions, then
    /// the config, the tracks and the reports at the same time (the tracks
    /// and reports decoded on their own threads), the boat list handed to
    /// `listed` as soon as the config is read. The reports only add
    /// official heading, speed, status and finish times, so a reports file
    /// that does not load or decode leaves those to be derived; a cancel or
    /// a tracker that keeps failing still ends the download.
    fn fetch_listed(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        listed: &mut dyn FnMut(TrackerEvent),
    ) -> Result<TrackerEvent> {
        self.fetch_inner(event, fetcher, progress, listed, false)
    }
}

impl Geovoile {
    fn fetch_inner(
        &self,
        event: &EventRef,
        fetcher: &Fetcher,
        progress: &mut dyn FnMut(Progress),
        listed: &mut dyn FnMut(TrackerEvent),
        metadata_only: bool,
    ) -> Result<TrackerEvent> {
        // The key goes into requests, so it is parsed again, never trusted.
        let mut site = site(&format!("{HTTPS}{}", event.key))?;
        let step = |step: u32| {
            move |bytes: u64, total: Option<u64>| Progress {
                step,
                steps: STEPS,
                bytes,
                total,
                fallback: false,
            }
        };
        let no_such = |site: &Site| TrackerError::NoSuchEvent {
            tracker: TRACKER,
            key: site.key(),
        };
        let page = reqwest::Url::parse(&site.url()).map_err(|_| no_such(&site))?;

        let at = step(0);
        let html = fetcher
            .get(&self.request(&page), &mut |b, t| progress(at(b, t)))
            .map_err(|e| match e {
                TrackerError::Http { status: 404, .. } => no_such(&site),
                other => other,
            })?;
        let html = String::from_utf8_lossy(&html);
        if !html.to_ascii_lowercase().contains("<html") {
            // `vendeeglobe.geovoile.com/2024/tracker/` answers "Not available".
            return Err(no_such(&site));
        }
        let mut viewer = parse_viewer(&html)?;
        if let Some(leg) = site.leg {
            if leg > viewer.legs {
                return Err(no_such(&site));
            }
            viewer.leg = leg;
        } else if viewer.legs > 1 {
            // The page shows its current leg; the event is that leg.
            site.leg = Some(viewer.leg);
        }

        let at = step(1);
        let versions_path = if viewer.versions_url.is_empty() {
            viewer.resource_path("versions", now_version())
        } else {
            format!("{}?v={}", viewer.versions_url, now_version())
        };
        let versions_url = resource_url(&page, &versions_path)?;
        let text = fetcher.get(&self.request(&versions_url), &mut |b, t| progress(at(b, t)))?;
        let text = String::from_utf8_lossy(&text);
        let text = text.trim_start_matches('\u{feff}').trim();
        // Some sites answer an empty versions file (Route du Rhum 2018 and
        // 2022); a version is only a cache-buster, so 0 then serves.
        let versions = if text.is_empty() {
            BTreeMap::new()
        } else {
            parse_versions(text)?
        };
        let version = |kind: &str| versions.get(kind).copied().unwrap_or(0);

        let address = |kind: &str| -> Result<String> {
            let url = resource_url(&page, &viewer.resource_path(kind, version(kind)))?;
            Ok(self.request(&url))
        };
        // The tracks and reports start at once and decode on their own
        // threads while the config is read here.
        let seeds = viewer.seeds;
        let tracks = if metadata_only {
            None
        } else {
            Some(fetcher.spawn(address("tracks")?, None, move |bytes| {
                parse_tracks(&decode_text(&bytes, seeds, false)?)
            })?)
        };
        let reports = fetcher.spawn(address("reports")?, None, move |bytes| {
            Ok(decode_text(&bytes, seeds, false)
                .and_then(|text| parse_reports(&text))
                .ok())
        })?;
        let at = step(2);
        let config = fetcher.get(&address("config")?, &mut |b, t| progress(at(b, t)))?;
        let config = parse_config(&decode_text(&config, viewer.seeds, true)?)?;
        if config.boats.is_empty() {
            return Err(unsupported("the config lists no boats"));
        }
        listed(event_of(
            EventRef {
                tracker: Tracker::Geovoile,
                key: site.key(),
                url: site.url(),
            },
            &viewer,
            &config,
            Vec::new(),
            None,
        ));
        let at = step(3);
        let tracks = match tracks {
            Some(tracks) => tracks.wait(&mut |b, t| progress(at(b, t)))?,
            None => Vec::new(),
        };
        let at = step(4);
        let reports = match reports.wait(&mut |b, t| progress(at(b, t))) {
            Ok(reports) => reports,
            Err(e @ (TrackerError::Cancelled | TrackerError::Unavailable { .. })) => return Err(e),
            Err(_) => None,
        };
        fetcher.check()?;
        // Done, whether or not the reports came.
        progress(Progress {
            step: STEPS - 1,
            steps: STEPS,
            bytes: 1,
            total: Some(1),
            fallback: false,
        });
        let event = EventRef {
            tracker: Tracker::Geovoile,
            key: site.key(),
            url: site.url(),
        };
        Ok(event_of(event, &viewer, &config, tracks, reports.as_ref()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MODERN: Seeds = Seeds([0x7B_C495, 0x45_57FA, 0xD5_6AAF, 0xFF_8040]);

    /// A hand-built stream: skip 0; the length and literal bytes are XORed
    /// with the keystream computed here step by step from Appendix A.
    #[test]
    fn a_hand_built_stream_with_a_back_reference_decodes() {
        // Plain text "abcabcab": literals a, b, c then a copy of 5 from 3 back.
        let mut k = Keystream::new(MODERN, 0);
        let mut enc = |b: u8| {
            b ^ {
                let x = (k.x & 0xFF) as u8;
                k.step();
                x
            }
        };
        let mut buf = vec![0u8, enc(0), enc(0), enc(8)];
        // Flags: bits 7..5 literal, bit 4 copy; the rest unused. Position 4.
        buf.push(0b0001_0000 ^ 4 ^ 0xA3);
        buf.push(enc(b'a'));
        buf.push(enc(b'b'));
        buf.push(enc(b'c'));
        // len 5 → (5 - 3) << 4; offset 3 → stored 2.
        buf.push(enc(2 << 4));
        buf.push(enc(2));
        assert_eq!(decode_hwx(&buf, MODERN).expect("decodes"), b"abcabcab");
    }

    #[test]
    fn a_short_or_bad_stream_is_an_error_not_a_panic() {
        assert!(decode_hwx(&[], MODERN).is_err());
        assert!(decode_hwx(&[0, 1, 2], MODERN).is_err());
        // A back-reference as the very first item.
        let mut k = Keystream::new(MODERN, 0);
        let mut enc = |b: u8| {
            b ^ {
                let x = (k.x & 0xFF) as u8;
                k.step();
                x
            }
        };
        let buf = vec![
            0u8,
            enc(0),
            enc(0),
            enc(4),
            0b1000_0000 ^ 4 ^ 0xA3,
            enc(0x10),
            enc(5),
        ];
        let err = decode_hwx(&buf, MODERN).expect_err("points before the start");
        assert!(err.to_string().contains("byte 5"), "{err}");
    }

    #[test]
    fn versions_parse_with_bare_keys() {
        let v = parse_versions(
            "{config:20220308182522,tracks:20220308182529,reports:20220308182523,geoblog:0}",
        )
        .expect("parses");
        assert_eq!(v["tracks"], 20_220_308_182_529);
        assert_eq!(v["geoblog"], 0);
        assert!(parse_versions("config:1").is_err());
        let err = parse_versions("{config:1,tracks:x}").expect_err("bad entry");
        assert!(err.to_string().contains("entry 2"), "{err}");
    }

    #[test]
    fn resource_paths_follow_the_viewer_rule() {
        let mut v = Viewer {
            title: String::new(),
            root_url: "/2025/".to_owned(),
            resources_url: String::new(),
            versions_url: String::new(),
            legs: 1,
            leg: 1,
            seeds: MODERN,
        };
        assert_eq!(
            v.resource_path("tracks", 7),
            "/2025/tracker/resources/tracks/v7"
        );
        v.legs = 4;
        v.leg = 2;
        assert_eq!(
            v.resource_path("config", 7),
            "/2025/tracker/resources/leg2/config/v7"
        );
        v.resources_url = "https://static.geovoile.com/x/".to_owned();
        assert_eq!(
            v.resource_path("config", 7),
            "https://static.geovoile.com/x/leg2_tracker_config.hwx?v=7"
        );
    }

    #[test]
    fn a_page_without_seeds_or_rooturl_is_unsupported() {
        let err = parse_viewer("<html><title>Old</title><embed src=\"x.hwz\"></html>")
            .expect_err("flash");
        assert!(matches!(err, TrackerError::Legacy { .. }), "{err:?}");
        assert!(err.to_string().contains("Flash"), "{err}");
        let err = parse_viewer("<html><title>Old</title></html>").expect_err("2014");
        assert!(matches!(err, TrackerError::Legacy { .. }), "{err:?}");
        let err = parse_viewer("Not available").expect_err("not a page");
        assert!(matches!(err, TrackerError::Unsupported { .. }), "{err:?}");
        let err = seeds_from_html("<html>rooturl :'/2014/'</html>").expect_err("no seeds");
        assert!(
            err.to_string().contains("unsupported Geovoile version"),
            "{err}"
        );
    }

    /// `nblegs` beyond 99 is clamped rather than trusted, and a `numleg`
    /// outside `1..=nblegs` is an unsupported page, not a wrong leg opened
    /// silently (M11 review carry).
    #[test]
    fn nblegs_is_clamped_and_numleg_out_of_range_is_refused() {
        let function = "window._0Xedc3=function(n){var a=0x88FE88;var b=0xFE88AA;var c=0xEECC80;var d=0xA0A0F0;for(;;){}; 0xFFFFFF}";
        let seg = base64::engine::general_purpose::STANDARD.encode(function);
        let page = |nblegs: &str, numleg: &str| {
            format!(
                "<html><title>T</title><img src=\"data:image/png;base64,iVBORw0KGgo=/C/{seg}\">rooturl :'/2025/' nblegs :{nblegs} numleg :{numleg}</html>"
            )
        };
        let v = parse_viewer(&page("500", "3")).expect("clamps");
        assert_eq!(v.legs, 99);
        let err = parse_viewer(&page("3", "5")).expect_err("beyond nblegs");
        assert!(matches!(err, TrackerError::Unsupported { .. }), "{err:?}");
        let err = parse_viewer(&page("3", "0")).expect_err("zero");
        assert!(matches!(err, TrackerError::Unsupported { .. }), "{err:?}");
    }

    /// Seeds as hex literals in the keystream function (the older form) are
    /// read in `var` order.
    #[test]
    fn literal_seeds_are_read_in_var_order() {
        let function = "window._0Xedc3=function(n){var a=0x88FE88;var b=0xFE88AA;var c=0xEECC80;var d=0xA0A0F0;for(;;){}; 0xFFFFFF}";
        let seg = base64::engine::general_purpose::STANDARD.encode(function);
        let html = format!("<img src=\"data:image/png;base64,iVBORw0KGgo=/C/{seg}\">");
        assert_eq!(
            seeds_from_html(&html).expect("parses"),
            Seeds([0x88_FE88, 0xFE_88AA, 0xEE_CC80, 0xA0_A0F0])
        );
    }

    #[test]
    /// Review fix: hostile deltas overflow or leave the globe; both are an
    /// unsupported-version error, never a panic or an imported fix.
    fn hostile_deltas_are_refused_not_imported() {
        for json in [
            format!(r#"{{"tracks":[{{"id":1,"loc":[[1758966938,4700000,-300000],[{},0,0]]}}]}}"#, i64::MAX),
            format!(r#"{{"tracks":[{{"id":1,"loc":[[1758966938,4700000,-300000],[60,{},0]]}}]}}"#, i64::MAX),
            r#"{"tracks":[{"id":1,"loc":[[1758966938,4700000,-300000],[60,5000000,0]]}]}"#.to_owned(),
            r#"{"tracks":[{"id":1,"loc":[[1758966938,4700000,-300000],[60,0,0],[-900000000,0,0]]}]}"#.to_owned(),
        ] {
            let err = parse_tracks(&json).expect_err(&json);
            assert!(err.to_string().contains("unsupported Geovoile version"), "{err}");
        }
        // A huge but finite longitude wraps into [-180, 180).
        let ok = parse_tracks(
            r#"{"tracks":[{"id":1,"loc":[[1758966938,4700000,-300000],[60,0,-72000000]]}]}"#,
        )
        .expect("wraps");
        let lon = ok[0].fixes[1].lon;
        assert!(
            (-180.0..180.0).contains(&lon) && (lon - -3.0).abs() < 1e-9,
            "{lon}"
        );
    }

    #[test]
    fn a_track_with_an_implausible_first_fix_is_refused() {
        let err = parse_tracks(r#"{"tracks":[{"id":1,"loc":[[12,4700000,-300000]]}]}"#)
            .expect_err("1970");
        assert!(err.to_string().contains("unsupported"), "{err}");
        let err = parse_tracks(r#"{"tracks":[{"id":1,"loc":[[1758966938,4700000]]}]}"#)
            .expect_err("short");
        assert!(err.to_string().contains("entry 0"), "{err}");
    }

    #[test]
    fn viewer_addresses_resolve_to_host_root_and_leg() {
        for input in [
            "https://vendeeglobe.geovoile.com/2016/tracker/",
            "http://vendeeglobe.geovoile.com/2016/viewer/",
            " vendeeglobe.geovoile.com/2016/tracker/index.html ",
            "https://vendeeglobe.geovoile.com/2016/",
            &format!("{HTTPS}VendeeGlobe.Geovoile.com/2016/index.html"),
        ] {
            let site = site(input).expect(input);
            assert_eq!(site.host, "vendeeglobe.geovoile.com", "{input}");
            assert_eq!(site.root, "/2016/", "{input}");
            assert_eq!(site.leg, None, "{input}");
        }
        let solitaire = site("https://lasolitaire.geovoile.com/2024/tracker/?leg=2").expect("leg");
        assert_eq!(solitaire.leg, Some(2));
        assert_eq!(solitaire.key(), "lasolitaire.geovoile.com/2024/?leg=2");
        assert_eq!(
            solitaire.url(),
            "https://lasolitaire.geovoile.com/2024/tracker/?leg=2"
        );
        // The key parses back to the same site (fetch re-reads it).
        assert_eq!(
            site(&format!("{HTTPS}{}", solitaire.key())).expect("key"),
            solitaire
        );
        let deep = site("https://x.geovoile.com/race/2023/tracker/").expect("two segments");
        assert_eq!(deep.root, "/race/2023/");
    }

    #[test]
    fn other_hosts_look_alikes_and_junk_are_refused() {
        for input in [
            "https://geovoile.com.evil.invalid/2016/tracker/",
            &format!("{HTTPS}evilgeovoile.com/2016/tracker/"),
            &format!("{HTTPS}user@vendeeglobe.geovoile.com/2016/tracker/"),
            "https://vendeeglobe.geovoile.com@evil.invalid/2016/tracker/",
            "https://vendeeglobe.geovoile.com:8443/2016/tracker/",
            "https://evil.invalid/2016/tracker/",
            "https://vendeeglobe.geovoile.com/",
            "https://vendeeglobe.geovoile.com/tracker/",
            "https://vendeeglobe.geovoile.com/2016/tracker/?leg=0",
            "https://vendeeglobe.geovoile.com/2016/tracker/?leg=x",
            "https://x.geovoile.com/%2e%2e/tracker/",
            "https://x.geovoile.com/a%20b/tracker/",
            "ftp://vendeeglobe.geovoile.com/2016/tracker/",
            "",
            "fastnet2025",
        ] {
            let err = site(input).expect_err(input);
            assert!(
                matches!(err, TrackerError::NotAnEvent { .. }),
                "{input}: {err:?}"
            );
        }
        let err = site("https://www.geovoile.com/vendee2008/tracker.hwz").expect_err("flash");
        assert!(matches!(err, TrackerError::Legacy { .. }), "{err:?}");
    }

    #[test]
    fn resources_resolve_only_to_geovoile_over_https() {
        let page = reqwest::Url::parse("https://x.geovoile.com/2025/tracker/").expect("a page");
        assert_eq!(
            resource_url(&page, "/2025/tracker/resources/config/v1")
                .expect("same host")
                .as_str(),
            "https://x.geovoile.com/2025/tracker/resources/config/v1"
        );
        assert!(
            resource_url(
                &page,
                "https://static.geovoile.com/a/tracker_config.hwx?v=1"
            )
            .is_ok()
        );
        for bad in [
            "https://evil.invalid/a",
            "//evil.invalid/a",
            "http://static.geovoile.com/a",
            &format!("{HTTPS}u:p@static.geovoile.com/a"),
            "https://static.geovoile.com:444/a",
            "https://static.geovoile.com.evil.invalid/a",
        ] {
            assert!(resource_url(&page, bad).is_err(), "{bad}");
        }
    }

    /// Hand-built: fixes a minute apart; reports at 0 s (heading 90, 10 kn),
    /// 130 s (nearest fix 120 s, heading 0 = none, 5 kn) and 1000 s (no fix
    /// within 60 s).
    #[test]
    fn reports_fill_the_nearest_fix_when_non_zero() {
        let mut fixes: Vec<Fix> = (0..4).map(|k| fix(1_700_000_000 + 60 * k, 0, 0)).collect();
        let line = |dt: i64, heading: f64, speed: f64| ReportLine {
            boat: 1,
            t: 1_700_000_000 + dt,
            status: None,
            heading,
            speed,
        };
        let lines = [
            line(0, 90.0, 10.0),
            line(130, 0.0, 5.0),
            line(1000, 45.0, 3.0),
        ];
        apply_reports(&mut fixes, &lines.iter().collect::<Vec<_>>());
        assert_eq!((fixes[0].cog, fixes[0].sog), (Some(90.0), Some(10.0)));
        assert_eq!((fixes[1].cog, fixes[1].sog), (None, None));
        assert_eq!((fixes[2].cog, fixes[2].sog), (None, Some(5.0)));
        assert_eq!((fixes[3].cog, fixes[3].sog), (None, None));
    }

    #[test]
    fn reports_without_their_columns_are_an_error() {
        assert!(parse_reports(r#"{"reports":{"columns":["boat"],"history":[]}}"#).is_err());
        let err = parse_reports("{\n\"reports\": }").expect_err("bad JSON");
        assert!(err.to_string().contains("line 2"), "{err}");
        let ok = parse_reports(
            r#"{"reports":{"columns":["speed","boat","heading","racestatus"],
               "history":[{"date":"2025-01-01T00:00:00Z","lines":[[12.5,"7",180,"RAC"],[1,"x",1,"RAC"]]}],
               "arrivals":[["7","2025-01-02T00:00:00Z",1]]}}"#,
        )
        .expect("parses");
        assert_eq!(ok.lines.len(), 1, "a line with no boat id is skipped");
        assert_eq!(
            (ok.lines[0].boat, ok.lines[0].heading, ok.lines[0].speed),
            (7, 180.0, 12.5)
        );
        assert_eq!(ok.arrivals[&7], 1_735_776_000);
    }
}
