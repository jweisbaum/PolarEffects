//! Native discovery and course extraction ported from SYRF tracker-scraper.
//! References: new-scrapers/{yellowbrick,geovoile_modern,bluewater}_scraper.js.
//! Public tracks only; configured YellowBrick credentials resolve free catalogue entries.
//! No browser or runtime sidecars.
pub mod completion;
pub mod yellowbrick;
use crate::{EventRef, Fetcher, TrackerError, TrackerEvent, error::Result, event};
use pe_core::track::Tracker;
use serde_json::{Value, json};
use std::collections::BTreeSet;

pub fn resolve_source(source: &str, input: &str) -> Result<EventRef> {
    let tracker = match source {
        "YELLOWBRICK" => Tracker::YellowBrick,
        "BLUEWATER" => Tracker::BlueWaterTracks,
        _ => Tracker::Geovoile,
    };
    event::client(tracker)
        .ok_or_else(|| decode("Tracker unavailable"))?
        .resolve(input)
}
pub fn resolve(input: &str) -> Result<EventRef> {
    if !input.contains('/') {
        return resolve_source("YELLOWBRICK", input);
    }

    for tracker in [
        Tracker::Geovoile,
        Tracker::BlueWaterTracks,
        Tracker::YellowBrick,
    ] {
        if let Some(client) = event::client(tracker)
            && let Ok(event) = client.resolve(input)
        {
            return Ok(event);
        }
    }
    Err(TrackerError::NotAnEvent {
        tracker: "SYRF",
        input: input.into(),
    })
}
pub fn source(tracker: Tracker) -> &'static str {
    match tracker {
        Tracker::YellowBrick => "YELLOWBRICK",
        Tracker::Geovoile => "GEOVOILE",
        Tracker::BlueWaterTracks => "BLUEWATER",
    }
}

pub fn discover(fetcher: &Fetcher, tracker: Tracker) -> Result<Vec<String>> {
    let mut urls = BTreeSet::new();
    match tracker {
        Tracker::YellowBrick => {
            return Ok(
                yellowbrick::discover(fetcher, None, &Default::default(), &mut |_| {})?.urls(),
            );
        }
        Tracker::BlueWaterTracks => {
            let bytes=fetcher.get("https://api.bluewatertracks.com/api/racelist/2012-01-01T00:00:00.000Z/2100-01-01T00:00:00.000Z",&mut|_,_|{})?;
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| decode(e.to_string()))?;
            let races = value["raceList"]
                .as_array()
                .ok_or_else(|| decode("No raceList"))?;
            for r in races {
                if let Some(slug) = r["slug"].as_str() {
                    let url = format!("{}/{slug}", crate::bluewater::SITE);
                    if resolve(&url).is_ok() {
                        urls.insert(url);
                    }
                }
            }
        }
        Tracker::Geovoile => {
            let root = "https://www.geovoile.com/";
            let bytes = fetcher.get(root, &mut |_, _| {})?;
            let html = String::from_utf8_lossy(&bytes);
            let links = links(root, &html);
            let archives: BTreeSet<_> = links
                .iter()
                .filter(|u| u.contains("/archives/") || u.contains("archives_20"))
                .cloned()
                .collect();
            add_races(&mut urls, &links);
            for page in archives {
                fetcher.check()?;
                let bytes = fetcher.get(&page, &mut |_, _| {})?;
                add_races(&mut urls, &links_of(&page, &bytes));
            }
        }
    }
    Ok(urls.into_iter().collect())
}
fn links(base: &str, html: &str) -> Vec<String> {
    let Ok(base) = reqwest::Url::parse(base) else {
        return vec![];
    };
    dom_query::Document::from(html)
        .select("a[href]")
        .iter()
        .filter_map(|n| base.join(&n.attr("href")?).ok())
        .filter(|u| crate::net::is_geovoile_host(u.host_str().unwrap_or_default()))
        .map(|mut u| {
            let _ = u.set_scheme("https");
            u.to_string()
        })
        .collect()
}
fn links_of(base: &str, bytes: &[u8]) -> Vec<String> {
    links(base, &String::from_utf8_lossy(bytes))
}
fn add_races(out: &mut BTreeSet<String>, links: &[String]) {
    for url in links {
        if reqwest::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_owned))
            .is_some_and(|h| h != "www.geovoile.com" && h != "geovoile.com")
            && resolve(url).is_ok()
        {
            out.insert(url.clone());
        }
    }
}
fn decode(why: impl Into<String>) -> TrackerError {
    TrackerError::Decode {
        what: "SYRF library",
        at: "response".into(),
        why: why.into(),
    }
}

/// Geometry-only Features are stored verbatim in CourseUnsequencedUntimedGeometries.
pub fn course(fetcher: &Fetcher, event: &mut TrackerEvent) -> Result<Vec<Value>> {
    match event.event.tracker {
        Tracker::YellowBrick => {
            let bytes = fetcher.get(
                &format!(
                    "{}/JSON/{}/RaceSetup",
                    crate::yellowbrick::CDN,
                    event.event.key
                ),
                &mut |_, _| {},
            )?;
            let text: String = bytes.iter().map(|b| char::from(*b)).collect();
            let v: Value = serde_json::from_str(&text).map_err(|e| decode(e.to_string()))?;
            if let Some(canonical) = v["url"].as_str()
                && let Ok(resolved) = resolve_source("YELLOWBRICK", canonical)
            {
                event.event = resolved;
            }
            yellowbrick_course(&v)
        }
        Tracker::BlueWaterTracks => {
            let bytes = fetcher.get(
                &format!("{}/api/race/{}", crate::bluewater::API, event.event.key),
                &mut |_, _| {},
            )?;
            let v: Value = serde_json::from_slice(&bytes).map_err(|e| decode(e.to_string()))?;
            let mut result = Vec::new();
            for key in ["startLine", "finishLine", "course"] {
                let f = &v["race"]["map"][key];
                if f["geometry"]["coordinates"].is_array() {
                    let mut f = f.clone();
                    f["properties"]["name"] = json!(key);
                    result.push(f);
                }
            }
            if let Some(regions) = v["race"]["map"]["regions"]["features"].as_array() {
                result.extend(regions.iter().cloned());
            }
            Ok(result)
        }
        Tracker::Geovoile => {
            let bytes = fetcher.get(&event.event.url, &mut |_, _| {})?;
            let viewer = crate::geovoile::parse_viewer(&String::from_utf8_lossy(&bytes))?;
            let page = reqwest::Url::parse(&event.event.url).map_err(|e| decode(e.to_string()))?;
            let url = crate::geovoile::resource_url(&page, &viewer.resource_path("config", 0))?;
            let config = fetcher.get(url.as_str(), &mut |_, _| {})?;
            let xml = crate::geovoile::decode_text(&config, viewer.seeds, true)?;
            geovoile_course(&xml)
        }
    }
}
pub fn yellowbrick_course(v: &Value) -> Result<Vec<Value>> {
    let mut result = Vec::new();
    if let Some(nodes) = v["course"]["nodes"].as_array() {
        for node in nodes {
            if node["lon"].is_number() && node["lat"].is_number() {
                result.push(json!({"type":"Feature","geometry":{"type":"Point","coordinates":[node["lon"],node["lat"]]},"properties":node}));
            }
        }
    }
    if let Some(lines) = v["poi"]["lines"].as_array() {
        for line in lines {
            if let Some(nodes) = line["nodes"].as_str() {
                let numbers: Vec<f64> = nodes
                    .split(',')
                    .map(|v| v.trim().parse::<f64>().map_err(|e| decode(e.to_string())))
                    .collect::<Result<_>>()?;
                if !numbers.len().is_multiple_of(2) {
                    return Err(decode("Odd coordinate count in YellowBrick POI"));
                }
                let mut coords: Vec<Value> = numbers
                    .chunks_exact(2)
                    .map(|p| json!([p[1], p[0]]))
                    .collect();
                let polygon = line["polygon"] == true;
                if coords.len() > 1 {
                    if polygon && coords.first() != coords.last() {
                        coords.push(coords[0].clone());
                    }
                    result.push(json!({"type":"Feature","geometry":{"type":if polygon{"Polygon"}else{"LineString"},"coordinates":if polygon{json!([coords])}else{json!(coords)}},"properties":line}));
                }
            }
        }
    }
    Ok(result)
}
pub fn geovoile_course(xml: &str) -> Result<Vec<Value>> {
    use quick_xml::events::Event;
    let mut result = Vec::new();
    let mut reader = quick_xml::Reader::from_str(xml);
    loop {
        let event = reader.read_event().map_err(|e| decode(e.to_string()))?;
        let open = matches!(event, Event::Start(_));
        let e = match event {
            Event::Start(e) | Event::Empty(e) => e,
            Event::Eof => break,
            _ => continue,
        };
        let name = e.name().into_inner().to_owned();
        let attrs: std::collections::BTreeMap<String, String> = e
            .attributes()
            .flatten()
            .filter_map(|a| {
                a.normalized_value(quick_xml::XmlVersion::Explicit1_0)
                    .ok()
                    .map(|v| (a.key.into_inner().to_owned(), v.into_owned()))
            })
            .collect();
        let num = |k: &str| attrs.get(k).and_then(|v| v.parse::<f64>().ok());
        if ["point", "start", "arrival", "mark", "buoy"].contains(&name.as_str()) {
            if let (Some(lat), Some(lon)) = (num("lat"), num("lng").or_else(|| num("lon"))) {
                result.push(json!({"type":"Feature","geometry":{"type":"Point","coordinates":[lon,lat]},"properties":attrs}));
            }
        } else if open && ["gate", "route", "racearea"].contains(&name.as_str()) {
            let text = reader
                .read_text(e.name())
                .map_err(|e| decode(e.to_string()))?;
            for part in text.split('|') {
                let mut coords: Vec<Value> = part
                    .split(';')
                    .filter(|s| !s.trim().is_empty())
                    .map(|s| {
                        let (lat, lon) = s
                            .trim()
                            .split_once(',')
                            .ok_or_else(|| decode("Bad Geovoile coordinate pair"))?;
                        let lat = lat.parse::<f64>().map_err(|e| decode(e.to_string()))?;
                        let lon = lon.parse::<f64>().map_err(|e| decode(e.to_string()))?;
                        Ok(json!([lon, lat]))
                    })
                    .collect::<Result<_>>()?;
                if coords.len() < 2 {
                    continue;
                }
                let polygon = name == "racearea";
                if polygon && coords.first() != coords.last() {
                    coords.push(coords[0].clone());
                }
                let mut properties = json!(attrs);
                properties["kind"] = json!(name);
                result.push(json!({"type":"Feature","geometry":{"type":if polygon{"Polygon"}else{"LineString"},"coordinates":if polygon{json!([coords])}else{json!(coords)}},"properties":properties}));
            }
        }
    }
    Ok(result)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn yellowbrick_preserves_pois_and_exclusion_polygons() {
        let bytes = include_bytes!("../tests/fixtures/yellowbrick/rmsr2024-RaceSetup.json");
        let text: String = bytes.iter().map(|b| char::from(*b)).collect();
        let v: Value = serde_json::from_str(&text).unwrap();
        let geometries = yellowbrick_course(&v).unwrap();
        let zone = geometries
            .iter()
            .find(|g| g["properties"]["name"] == "Stromboli Exclusion Zone")
            .unwrap();
        assert_eq!(zone["geometry"]["type"], "Polygon");
        assert_eq!(
            zone["geometry"]["coordinates"][0][0],
            json!([15.18917, 38.79583])
        );
        assert_eq!(
            zone["geometry"]["coordinates"][0]
                .as_array()
                .unwrap()
                .first(),
            zone["geometry"]["coordinates"][0]
                .as_array()
                .unwrap()
                .last()
        );
    }
    #[test]
    fn geovoile_decodes_recorded_routes_gates_and_areas() {
        let html = include_str!("../tests/fixtures/geovoile/24hultim2025/viewer.html");
        let v = crate::geovoile::parse_viewer(html).unwrap();
        let xml = crate::geovoile::decode_text(
            include_bytes!("../tests/fixtures/geovoile/24hultim2025/config.hwx"),
            v.seeds,
            true,
        )
        .unwrap();
        let g = geovoile_course(&xml).unwrap();
        let gate = g
            .iter()
            .find(|g| g["properties"]["name"] == "ArrivalLine")
            .unwrap();
        assert_eq!(
            gate["geometry"]["coordinates"],
            json!([[-3.461, 47.6995], [-3.45667, 47.69]])
        );
        assert_eq!(
            g.iter()
                .filter(|g| g["properties"]["kind"] == "route")
                .count(),
            2
        );
        assert!(g.iter().any(|g| g["geometry"]["type"] == "Polygon"));
    }
}
