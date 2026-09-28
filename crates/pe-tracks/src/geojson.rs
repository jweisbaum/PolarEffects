//! GeoJSON tracks (spec.md 7.3).
//!
//! A FeatureCollection (or a single Feature) of Point features, each one
//! fix, or LineString features with a time per vertex in
//! `properties.times` or `properties.coordTimes` (MultiLineString too, with
//! one list of times per line, as GPX converters write). Property names are
//! matched without regard to case. Features are grouped into one track per
//! boat name, in the order the boats first appear.

use pe_core::track::Fix;
use serde_json::{Map, Value};

use crate::error::{Reason, Result, TrackFileError};
use crate::time::{TimeFormat, parse_time};
use crate::{MAX_FIXES, RawTrack, check_heading, check_speed, position};

const TIME_KEYS: [&str; 3] = ["time", "timestamp", "date"];
const HEADING_KEYS: [&str; 4] = ["cog", "heading", "hdg", "course"];
const SPEED_KEYS: [&str; 4] = ["sog", "speed", "bsp", "stw"];
const BOAT_KEYS: [&str; 2] = ["boat", "name"];
const LINE_TIME_KEYS: [&str; 2] = ["times", "coordtimes"];

/// A property by any of `keys`, ignoring case; the first key that matches
/// wins.
fn property<'a>(properties: Option<&'a Map<String, Value>>, keys: &[&str]) -> Option<&'a Value> {
    let properties = properties?;
    keys.iter().find_map(|key| {
        properties
            .iter()
            .find(|(name, value)| name.eq_ignore_ascii_case(key) && !value.is_null())
            .map(|(_, value)| value)
    })
}

fn time_of(value: &Value, feature: usize) -> Result<i64> {
    let text = match value {
        Value::String(s) => s.clone(),
        Value::Number(n) => n.to_string(),
        _ => return Err(TrackFileError::in_feature(feature, Reason::NoTime)),
    };
    parse_time(&text, &TimeFormat::Auto)
        .map_err(|e| TrackFileError::in_feature(feature, Reason::BadTime(e)))
}

fn number_of(value: Option<&Value>, feature: usize) -> Result<Option<f64>> {
    match value {
        None => Ok(None),
        Some(Value::Number(n)) => Ok(n.as_f64()),
        Some(Value::String(s)) => s
            .trim()
            .parse::<f64>()
            .map(Some)
            .map_err(|_| TrackFileError::in_feature(feature, Reason::NotANumber(s.clone()))),
        Some(other) => Err(TrackFileError::in_feature(
            feature,
            Reason::NotANumber(other.to_string()),
        )),
    }
}

fn lon_lat(value: &Value, feature: usize) -> Result<(f64, f64)> {
    let bad = || TrackFileError::in_feature(feature, Reason::BadPosition);
    let pair = value.as_array().ok_or_else(bad)?;
    let lon = pair.first().and_then(Value::as_f64).ok_or_else(bad)?;
    let lat = pair.get(1).and_then(Value::as_f64).ok_or_else(bad)?;
    position(lat, lon).ok_or_else(bad)
}

/// One boat's fixes while the file is read.
struct Group {
    boat: Option<String>,
    fixes: Vec<Fix>,
}

fn group_for(groups: &mut Vec<Group>, boat: Option<String>) -> &mut Group {
    let index = match groups.iter().position(|g| g.boat == boat) {
        Some(index) => index,
        None => {
            groups.push(Group {
                boat,
                fixes: Vec::new(),
            });
            groups.len() - 1
        }
    };
    &mut groups[index]
}

fn read_feature(
    feature: &Value,
    index: usize,
    groups: &mut Vec<Group>,
    total: &mut usize,
) -> Result<()> {
    let not_geojson = || TrackFileError::in_feature(index, Reason::NotGeoJson);
    let geometry = feature.get("geometry").ok_or_else(not_geojson)?;
    if geometry.is_null() {
        // A feature with no geometry (a legend, a note) is not a position.
        return Ok(());
    }
    let properties = feature.get("properties").and_then(Value::as_object);
    let kind = geometry
        .get("type")
        .and_then(Value::as_str)
        .ok_or_else(not_geojson)?;
    let coordinates = geometry.get("coordinates").ok_or_else(not_geojson)?;
    let boat = property(properties, &BOAT_KEYS).map(|v| match v {
        Value::String(s) => s.trim().to_owned(),
        other => other.to_string(),
    });
    let boat = boat.filter(|b| !b.is_empty());

    let mut fixes = Vec::new();
    match kind {
        "Point" => {
            let (lat, lon) = lon_lat(coordinates, index)?;
            let time = property(properties, &TIME_KEYS)
                .ok_or_else(|| TrackFileError::in_feature(index, Reason::NoTime))?;
            let t = time_of(time, index)?;
            let cog = number_of(property(properties, &HEADING_KEYS), index)?;
            let sog = number_of(property(properties, &SPEED_KEYS), index)?;
            let range = |r| TrackFileError::in_feature(index, r);
            fixes.push(Fix {
                t,
                lat,
                lon,
                cog: check_heading(cog).map_err(range)?,
                sog: check_speed(sog).map_err(range)?,
            });
        }
        "LineString" | "MultiLineString" => {
            let times = property(properties, &LINE_TIME_KEYS)
                .and_then(Value::as_array)
                .ok_or_else(|| TrackFileError::in_feature(index, Reason::NoTime))?;
            let lines: Vec<(&Vec<Value>, &Vec<Value>)> = if kind == "LineString" {
                vec![(coordinates.as_array().ok_or_else(not_geojson)?, times)]
            } else {
                let parts = coordinates.as_array().ok_or_else(not_geojson)?;
                if parts.len() != times.len() {
                    return Err(TrackFileError::in_feature(
                        index,
                        Reason::TimesMismatch {
                            vertices: parts.len(),
                            times: times.len(),
                        },
                    ));
                }
                parts
                    .iter()
                    .zip(times)
                    .map(|(p, t)| Some((p.as_array()?, t.as_array()?)))
                    .collect::<Option<_>>()
                    .ok_or_else(not_geojson)?
            };
            for (vertices, times) in lines {
                if vertices.len() != times.len() {
                    return Err(TrackFileError::in_feature(
                        index,
                        Reason::TimesMismatch {
                            vertices: vertices.len(),
                            times: times.len(),
                        },
                    ));
                }
                for (vertex, time) in vertices.iter().zip(times) {
                    let (lat, lon) = lon_lat(vertex, index)?;
                    fixes.push(Fix {
                        t: time_of(time, index)?,
                        lat,
                        lon,
                        cog: None,
                        sog: None,
                    });
                }
            }
        }
        other => {
            return Err(TrackFileError::in_feature(
                index,
                Reason::UnsupportedGeometry(other.to_owned()),
            ));
        }
    }
    *total += fixes.len();
    if *total > MAX_FIXES {
        return Err(TrackFileError::whole(Reason::TooManyFixes));
    }
    group_for(groups, boat).fixes.extend(fixes);
    Ok(())
}

/// Reads a GeoJSON file into one raw track per boat.
pub fn read_geojson(bytes: &[u8]) -> Result<Vec<RawTrack>> {
    if bytes.iter().all(u8::is_ascii_whitespace) {
        return Err(TrackFileError::whole(Reason::Empty));
    }
    let root: Value = serde_json::from_slice(bytes).map_err(|e| TrackFileError {
        line: Some(e.line().max(1)),
        column: Some(e.column().max(1)),
        feature: None,
        reason: Reason::NotJson(e.to_string()),
    })?;
    let features: Vec<&Value> = match root.get("type").and_then(Value::as_str) {
        Some("FeatureCollection") => root
            .get("features")
            .and_then(Value::as_array)
            .ok_or_else(|| TrackFileError::whole(Reason::NotGeoJson))?
            .iter()
            .collect(),
        Some("Feature") => vec![&root],
        _ => return Err(TrackFileError::whole(Reason::NotGeoJson)),
    };
    let mut groups = Vec::new();
    let mut total = 0;
    for (index, feature) in features.iter().enumerate() {
        read_feature(feature, index, &mut groups, &mut total)?;
    }
    let tracks: Vec<RawTrack> = groups
        .into_iter()
        .filter(|g| !g.fixes.is_empty())
        .map(|g| RawTrack {
            boat: g.boat,
            fixes: g.fixes,
        })
        .collect();
    if tracks.is_empty() {
        return Err(TrackFileError::whole(Reason::NoFixes));
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOON: i64 = 1_753_531_200;

    #[test]
    fn points_grouped_by_boat_with_any_case_properties() {
        let text = r#"{"type":"FeatureCollection","features":[
          {"type":"Feature","geometry":{"type":"Point","coordinates":[-1.3,50.1]},
           "properties":{"Time":"2025-07-26T12:00:00Z","Boat":"Alpha","COG":45,"SOG":"7.5"}},
          {"type":"Feature","geometry":{"type":"Point","coordinates":[-1.2,50.2]},
           "properties":{"timestamp":1753531260,"name":"Bravo","heading":90}},
          {"type":"Feature","geometry":{"type":"Point","coordinates":[-1.25,50.15]},
           "properties":{"DATE":1753531320000,"boat":"Alpha","bsp":6.0}},
          {"type":"Feature","geometry":null,"properties":{"note":"legend"}}
        ]}"#;
        let tracks = read_geojson(text.as_bytes()).unwrap();
        assert_eq!(tracks.len(), 2);
        assert_eq!(tracks[0].boat.as_deref(), Some("Alpha"));
        assert_eq!(tracks[0].fixes.len(), 2);
        assert_eq!(
            tracks[0].fixes[0],
            Fix {
                t: NOON,
                lat: 50.1,
                lon: -1.3,
                cog: Some(45.0),
                sog: Some(7.5)
            }
        );
        assert_eq!(tracks[0].fixes[1].t, NOON + 120);
        assert_eq!(tracks[0].fixes[1].sog, Some(6.0));
        assert_eq!(tracks[1].boat.as_deref(), Some("Bravo"));
        assert_eq!(tracks[1].fixes[0].cog, Some(90.0));
    }

    #[test]
    fn lines_with_times_or_coord_times() {
        let text = r#"{"type":"Feature","properties":{"coordTimes":["2025-07-26T12:00:00Z","2025-07-26T12:01:00Z"]},
          "geometry":{"type":"LineString","coordinates":[[179.9,10,0],[-179.9,10,0]]}}"#;
        let tracks = read_geojson(text.as_bytes()).unwrap();
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].boat, None);
        assert_eq!(tracks[0].fixes[1].lon, -179.9);
        assert_eq!(tracks[0].fixes[1].t, NOON + 60);

        let multi = r#"{"type":"Feature","properties":{"TIMES":[[1753531200],[1753531260,1753531320]]},
          "geometry":{"type":"MultiLineString","coordinates":[[[0,0]],[[0,1],[180,2]]]}}"#;
        let tracks = read_geojson(multi.as_bytes()).unwrap();
        assert_eq!(tracks[0].fixes.len(), 3);
        // 180° is stored as -180° (CLAUDE.md: [-180, 180)).
        assert_eq!(tracks[0].fixes[2].lon, -180.0);
    }

    #[test]
    fn malformed_files_name_where() {
        let syntax =
            read_geojson(b"{\"type\": \"FeatureCollection\",\n  \"features\": [ }").unwrap_err();
        assert_eq!(syntax.line, Some(2));
        assert!(syntax.column.is_some());
        assert_eq!(syntax.reason.code(), "not-json");

        let cases: [(&str, &str, Option<usize>); 7] = [
            (r#"{"type":"Topology"}"#, "not-geojson", None),
            (
                r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{}}]}"#,
                "no-time",
                Some(0),
            ),
            (
                r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[0,95]},"properties":{"time":0}}]}"#,
                "bad-position",
                Some(0),
            ),
            (
                r#"{"type":"FeatureCollection","features":[{"type":"Feature","geometry":{"type":"Point","coordinates":[0,1]},"properties":{"time":"soon"}}]}"#,
                "bad-time",
                Some(0),
            ),
            (
                r#"{"type":"Feature","properties":{"times":[0]},"geometry":{"type":"LineString","coordinates":[[0,0],[1,1]]}}"#,
                "times-mismatch",
                Some(0),
            ),
            (
                r#"{"type":"Feature","properties":{},"geometry":{"type":"Polygon","coordinates":[]}}"#,
                "unsupported-geometry",
                Some(0),
            ),
            (
                r#"{"type":"FeatureCollection","features":[]}"#,
                "no-fixes",
                None,
            ),
        ];
        for (text, code, feature) in cases {
            let err = read_geojson(text.as_bytes()).unwrap_err();
            assert_eq!(err.reason.code(), code, "{text}");
            assert_eq!(err.feature, feature, "{text}");
        }
        let heading = r#"{"type":"Feature","geometry":{"type":"Point","coordinates":[0,0]},"properties":{"time":0,"cog":400}}"#;
        assert_eq!(
            read_geojson(heading.as_bytes()).unwrap_err().reason.code(),
            "out-of-range"
        );
        assert_eq!(read_geojson(b"  ").unwrap_err().reason, Reason::Empty);
    }
}
