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

use pe_core::track::Fix;

use crate::error::{Result, TrackerError};

const TRACKER: &str = "Geovoile";

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
    let segments = script_segments(html)?;
    let mut globals: BTreeMap<String, u32> = BTreeMap::new();
    let mut function: Option<&str> = None;
    for segment in &segments {
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

/// The decoded `/C/` script segments after the first `data:image/png`.
fn script_segments(html: &str) -> Result<Vec<String>> {
    const MARK: &str = "data:image/png;base64,";
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
            return Ok(segments);
        }
        search = &search[start + MARK.len()..];
    }
    Err(unsupported(
        "the viewer page carries no hwx seeds; only the tracker/ generation (about 2016 on) is supported",
    ))
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
    if html.contains(".hwz") && !html.contains("rooturl") {
        return Err(unsupported(
            "this is a Flash tracker (.hwz), which is not supported",
        ));
    }
    let root_url = param(html, "rooturl")
        .ok_or_else(|| unsupported("the viewer page has no rooturl"))?
        .to_owned();
    let number = |key: &str| param(html, key).and_then(|v| v.parse::<u32>().ok());
    let title = html
        .find("<title>")
        .and_then(|at| {
            let rest = &html[at + 7..];
            rest.find("</title>")
                .map(|end| rest[..end].trim().to_owned())
        })
        .unwrap_or_default();
    Ok(Viewer {
        title,
        root_url,
        resources_url: param(html, "resourcesurl").unwrap_or_default().to_owned(),
        legs: number("nblegs").unwrap_or(1),
        leg: number("numleg").unwrap_or(1),
        seeds: seeds_from_html(html)?,
    })
}

impl Viewer {
    /// Where a resource of `kind` (`config`, `tracks`, `reports`) at
    /// `version` is, relative to the site origin — the viewer's own rule.
    pub fn resource_path(&self, kind: &str, version: u64) -> String {
        let is_static = !self.resources_url.is_empty();
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
}

/// The parts of the config an import uses.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Config {
    /// Race name.
    pub name: String,
    /// The `date` attribute (race start), as written.
    pub date: Option<String>,
    /// Every boat.
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
    let mut config = Config {
        name: String::new(),
        date: None,
        boats: Vec::new(),
    };
    loop {
        let event = reader.read_event().map_err(|e| TrackerError::Decode {
            what: "Geovoile config",
            at: format!("byte {}", reader.buffer_position()),
            why: e.to_string(),
        })?;
        match event {
            Event::Start(e) | Event::Empty(e) => match e.name().into_inner() {
                "config" => {
                    let a = attrs(&e);
                    config.name = a.get("name").cloned().unwrap_or_default();
                    config.date = a.get("date").cloned();
                }
                "boat" => {
                    let a = attrs(&e);
                    let Some(id) = a.get("id").and_then(|v| v.parse().ok()) else {
                        continue;
                    };
                    let non_empty = |k: &str| a.get(k).filter(|v| !v.is_empty()).cloned();
                    config.boats.push(Boat {
                        id,
                        name: a.get("name").cloned().unwrap_or_default(),
                        sail: non_empty("sail"),
                        colour: non_empty("trackcolor")
                            .filter(|c| c.len() == 6 && c.chars().all(|ch| ch.is_ascii_hexdigit()))
                            .map(|c| format!("#{}", c.to_ascii_lowercase())),
                    });
                }
                _ => {}
            },
            Event::Eof => break,
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
        t,
        lat: lat as f64 / 1e5,
        lon: crate::wrap_lon(lon),
        cog: None,
        sog: None,
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
        assert!(err.to_string().contains("Flash"), "{err}");
        let err = seeds_from_html("<html>rooturl :'/2014/'</html>").expect_err("no seeds");
        assert!(
            err.to_string().contains("unsupported Geovoile version"),
            "{err}"
        );
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
}
