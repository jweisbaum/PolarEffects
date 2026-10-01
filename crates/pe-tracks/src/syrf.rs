//! SYRF race GeoJSON: participant/vessel GUID per LineString, [lon,lat,alt,time_ms].
use pe_core::track::Fix;
use serde_json::Value;
use std::collections::BTreeSet;

pub fn read(bytes: &[u8], identities: &BTreeSet<&str>) -> Result<Vec<Fix>, String> {
    let root: Value = serde_json::from_slice(bytes).map_err(|e| e.to_string())?;
    let features: Vec<&Value> = if root["type"] == "Feature" {
        vec![&root]
    } else {
        root["features"]
            .as_array()
            .ok_or("Expected a SYRF Feature or FeatureCollection")?
            .iter()
            .collect()
    };
    let mut fixes = Vec::new();
    let number = |v: &Value| v.as_f64().or_else(|| v.as_str()?.parse::<f64>().ok());
    for (fi, f) in features.iter().enumerate() {
        let id = f["properties"]["vesselParticipantId"]
            .as_str()
            .or_else(|| f["properties"]["id"].as_str())
            .unwrap_or_default();
        if !identities.contains(id) {
            continue;
        }
        if f["geometry"]["type"] != "LineString" {
            return Err(format!("Feature {fi}: expected LineString"));
        }
        let detail = &f["properties"]["detail"];
        let field_index = |field: &str, fallback: usize| {
            if detail.is_object() {
                detail[field].as_u64().map(|v| v as usize)
            } else {
                Some(fallback)
            }
        };
        let coords = f["geometry"]["coordinates"]
            .as_array()
            .ok_or("Missing coordinates")?;
        if fixes.len() + coords.len() > crate::MAX_FIXES {
            return Err("Too many track positions".into());
        }
        for (i, c) in coords.iter().enumerate() {
            let bad = || {
                format!(
                    "Feature {fi}, coordinate {i}: expected finite longitude, latitude, altitude and epoch milliseconds"
                )
            };
            let lon = number(&c[0]).ok_or_else(bad)?;
            let lat = number(&c[1]).ok_or_else(bad)?;
            let ms = number(&c[3]).ok_or_else(bad)?;
            if !ms.is_finite() || !(0.0..=253402300799000.0).contains(&ms) {
                return Err(bad());
            }
            let (lat, lon) = crate::position(lat, lon).ok_or_else(bad)?;
            let supplied = |names: &[&str]| -> Option<f64> {
                names.iter().find_map(|name| {
                    detail
                        .as_object()?
                        .iter()
                        .find(|(key, _)| key.eq_ignore_ascii_case(name))?
                        .1
                        .as_u64()
                        .and_then(|i| c.get(i as usize))
                        .and_then(number)
                })
            };
            let tws = crate::check_wind_speed(supplied(&["tws", "windSpeed", "trueWindSpeed"]))
                .map_err(|_| {
                    format!("Feature {fi}, coordinate {i}: invalid supplied wind speed")
                })?;
            let twd_from = crate::check_heading(supplied(&[
                "twd",
                "twd_from",
                "windDirection",
                "trueWindDirection",
            ]))
            .map_err(|_| {
                format!("Feature {fi}, coordinate {i}: invalid supplied wind direction")
            })?;
            fixes.push(Fix {
                tws,
                twd_from,
                t: (ms / 1000.0).floor() as i64,
                lat,
                lon,
                cog: field_index("cog", 5)
                    .and_then(|i| number(&c[i]))
                    .filter(|n| n.is_finite() && (0.0..360.0).contains(n)),
                sog: field_index("sog", 4)
                    .and_then(|i| number(&c[i]))
                    .filter(|n| n.is_finite() && *n >= 0.0),
            });
        }
    }
    if fixes.is_empty() {
        return Err("No coordinates matched the boat's GUID in this race file".into());
    }
    Ok(fixes)
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn selects_guid_and_reads_millisecond_coordinates() {
        let json=br#"{"type":"FeatureCollection","features":[{"properties":{"id":"other"},"geometry":{"type":"LineString","coordinates":[[0,0,0,0]]}},{"properties":{"id":"boat"},"geometry":{"type":"LineString","coordinates":[["-1.3","50.1",0,1753531200000],[180,50.2,0,1753531260999]]}}]}"#;
        let f = read(json, &BTreeSet::from(["boat"])).unwrap();
        assert_eq!(f.len(), 2);
        assert_eq!(f[0].t, 1753531200);
        assert_eq!(f[0].lon, -1.3);
        assert_eq!(f[1].lon, -180.0);
        assert_eq!(f[1].t, 1753531260);
        assert!(read(json, &BTreeSet::from(["missing"])).is_err());
    }
    #[test]
    fn malformed_coordinates_fail_without_panicking() {
        assert!(read(br#"{"features":[{"properties":{"id":"b"},"geometry":{"type":"LineString","coordinates":[[1,99,0,1000]]}}]}"#,&BTreeSet::from(["b"])).is_err());
    }
}

#[cfg(test)]
mod column_tests {
    use super::*;
    #[test]
    fn detail_map_controls_optional_column_order() {
        let bytes=br#"{"type":"Feature","properties":{"vesselParticipantId":"boat","detail":{"lon":0,"lat":1,"elevation":2,"time":3,"cog":4,"sog":5}},"geometry":{"type":"LineString","coordinates":[[1,50,0,1753531200000,270,7]]}}"#;
        let fixes = read(bytes, &BTreeSet::from(["boat"])).unwrap();
        assert_eq!(fixes[0].sog, Some(7.0));
        assert_eq!(fixes[0].cog, Some(270.0));
    }

    #[test]
    fn supplied_wind_requires_a_named_detail_column() {
        let bytes = br#"{"type":"Feature","properties":{"id":"boat","detail":{"twd":4,"tws":5}},"geometry":{"type":"LineString","coordinates":[[1,50,0,1753531200000,90,12]]}}"#;
        let f = read(bytes, &BTreeSet::from(["boat"])).unwrap();
        assert_eq!(f[0].tws.zip(f[0].twd_from), Some((12.0, 90.0)));
        assert_eq!(f[0].cog, None);
    }
}
