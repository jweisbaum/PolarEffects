//! A minimal, bounded reader for the KML YellowBrick serves as its fallback
//! (`GET https://yb.tl/<key>.kml`, spec.md 7.2).
//!
//! The file is one `Folder` of `Placemark`s, one per team, each named after
//! the team and holding a `gx:Track`: every `when` (ISO 8601, UTC) first,
//! then as many `gx:coord` (`lon,lat,alt`), oldest first. Only that is
//! read; styles, icons and every other element are skipped. Namespace
//! prefixes vary (`ns2:`, `kml:`, none), so elements are matched by their
//! local name.
//!
//! YellowBrick's own positions are integers in 1e-5 degrees, which the KML
//! writes through a float (`14.501920000000013`); each coordinate is rounded
//! back to 1e-5 so the fixes equal the binary's. The binary also carries a
//! few reports at a duplicate time which the KML leaves out.

use pe_core::track::Fix;
use pe_tracks::time::{TimeFormat, parse_time};
use quick_xml::events::Event;

use crate::error::{Result, TrackerError};

/// The most placemarks one file may hold. The largest fleets are a few
/// hundred boats.
pub const MAX_PLACEMARKS: usize = 20_000;

/// The most positions one file may hold, over all placemarks.
pub const MAX_POSITIONS: usize = 20_000_000;

/// One placemark's track.
#[derive(Debug, Clone, PartialEq)]
pub struct KmlTrack {
    /// The placemark's name: the team name.
    pub name: String,
    /// Positions in file order, longitude in [-180, 180).
    pub fixes: Vec<Fix>,
}

fn error(at: u64, why: String) -> TrackerError {
    TrackerError::Decode {
        what: "YellowBrick KML",
        at: format!("byte {at}"),
        why,
    }
}

/// A placemark being read: its name, times and (lat, lon) positions.
type Placemark = (String, Vec<i64>, Vec<(f64, f64)>);

/// Which text is being collected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Field {
    None,
    Name,
    When,
    Coord,
}

/// The text of the five predefined entities.
fn entity(name: &str) -> Option<&'static str> {
    Some(match name {
        "amp" => "&",
        "lt" => "<",
        "gt" => ">",
        "quot" => "\"",
        "apos" => "'",
        _ => return None,
    })
}

/// Rounds to YellowBrick's 1e-5 degree grid.
fn grid(value: f64) -> f64 {
    (value * 1e5).round() / 1e5
}

/// One `lon,lat[,alt]` coordinate as (lat, lon).
fn coordinate(text: &str) -> Option<(f64, f64)> {
    let mut parts = text.trim().split(',');
    let lon: f64 = parts.next()?.trim().parse().ok()?;
    let lat: f64 = parts.next()?.trim().parse().ok()?;
    let (lat, lon) = (grid(lat), grid(lon));
    (lat.is_finite()
        && lon.is_finite()
        && (-90.0..=90.0).contains(&lat)
        && (-360.0..=360.0).contains(&lon))
    .then(|| (lat, crate::wrap_lon(lon)))
}

/// Reads every placemark track of a YellowBrick KML.
///
/// # Errors
/// [`TrackerError::Decode`] naming the byte for malformed XML, an
/// unreadable time or coordinate, a track whose times and coordinates do
/// not pair up, or a file past the placemark or position bounds.
pub fn parse_tracks(bytes: &[u8]) -> Result<Vec<KmlTrack>> {
    let text = std::str::from_utf8(bytes)
        .map_err(|e| error(e.valid_up_to() as u64, "the file is not UTF-8".to_owned()))?;
    let mut reader = quick_xml::Reader::from_str(text);
    let mut tracks = Vec::new();
    let mut positions = 0usize;
    // The placemark being read: its name, times and coordinates.
    let mut current: Option<Placemark> = None;
    let mut field = Field::None;
    let mut buf = String::new();
    loop {
        let at = reader.buffer_position();
        let event = reader
            .read_event()
            .map_err(|e| error(reader.buffer_position(), e.to_string()))?;
        match event {
            Event::Start(e) => match e.local_name().into_inner() {
                "Placemark" => {
                    if current.is_some() {
                        return Err(error(at, "a placemark inside a placemark".to_owned()));
                    }
                    if tracks.len() >= MAX_PLACEMARKS {
                        return Err(error(at, format!("more than {MAX_PLACEMARKS} placemarks")));
                    }
                    current = Some((String::new(), Vec::new(), Vec::new()));
                }
                name if current.is_some() => {
                    field = match name {
                        "name" => Field::Name,
                        "when" => Field::When,
                        "coord" => Field::Coord,
                        _ => Field::None,
                    };
                    buf.clear();
                }
                _ => {}
            },
            Event::Text(t) if field != Field::None => buf.push_str(&t.xml10_content()),
            Event::CData(t) if field != Field::None => buf.push_str(&t.xml10_content()),
            Event::GeneralRef(r) if field != Field::None => {
                if let Some(c) = r.resolve_char_ref().ok().flatten() {
                    buf.push(c);
                } else if let Some(s) = entity(&r.xml10_content()) {
                    buf.push_str(s);
                }
            }
            Event::End(e) => {
                let local = e.local_name().into_inner().to_owned();
                if let Some((name, whens, coords)) = current.as_mut() {
                    match (local.as_str(), field) {
                        ("name", Field::Name) => *name = buf.trim().to_owned(),
                        ("when", Field::When) => {
                            let t = parse_time(&buf, &TimeFormat::Iso8601).map_err(|e| {
                                error(at, format!("the time {:?}: {e}", buf.trim()))
                            })?;
                            whens.push(t);
                        }
                        ("coord", Field::Coord) => {
                            let c = coordinate(&buf).ok_or_else(|| {
                                error(at, format!("{:?} is not a position", buf.trim()))
                            })?;
                            coords.push(c);
                            positions += 1;
                            if positions > MAX_POSITIONS {
                                return Err(error(
                                    at,
                                    format!("more than {MAX_POSITIONS} positions"),
                                ));
                            }
                        }
                        ("Placemark", _) => {
                            if let Some((name, whens, coords)) = current.take() {
                                if whens.len() != coords.len() {
                                    return Err(error(
                                        at,
                                        format!(
                                            "{name:?} has {} times for {} positions",
                                            whens.len(),
                                            coords.len()
                                        ),
                                    ));
                                }
                                let fixes = whens
                                    .into_iter()
                                    .zip(coords)
                                    .map(|(t, (lat, lon))| Fix {
                                        tws: None,
                                        twd_from: None,
                                        t,
                                        lat,
                                        lon,
                                        cog: None,
                                        sog: None,
                                    })
                                    .collect();
                                tracks.push(KmlTrack { name, fixes });
                            }
                        }
                        _ => {}
                    }
                }
                field = Field::None;
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if current.is_some() {
        return Err(error(
            reader.buffer_position(),
            "the file ends inside a placemark".to_owned(),
        ));
    }
    Ok(tracks)
}

#[cfg(test)]
mod tests {
    use super::*;

    const ONE: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<kml>
<Document><Folder><name>Tracks</name>
<Placemark><name>A &amp; B</name><styleUrl>#s</styleUrl>
<gx:Track><gx:altitudeMode>clampToGround</gx:altitudeMode>
<when>2024-10-18T06:33:25Z</when><when>2024-10-18T07:00:08Z</when>
<gx:coord>14.501920000000013,35.90196,28</gx:coord>
<gx:coord>190.5,-35.902100000000004,0</gx:coord>
</gx:Track></Placemark>
</Folder></Document></kml>"#;

    #[test]
    fn a_placemark_reads_to_fixes_on_the_grid() {
        let tracks = parse_tracks(ONE.as_bytes()).expect("reads");
        assert_eq!(tracks.len(), 1);
        assert_eq!(tracks[0].name, "A & B");
        let f = &tracks[0].fixes;
        assert_eq!(
            (f[0].t, f[0].lat, f[0].lon),
            (1_729_233_205, 35.90196, 14.50192)
        );
        // Past the antimeridian, folded; the latitude back on the grid.
        assert_eq!(
            (f[1].t, f[1].lat, f[1].lon),
            (1_729_234_808, -35.9021, -169.5)
        );
    }

    #[test]
    fn malformed_files_name_the_byte() {
        let unpaired = ONE.replace("<when>2024-10-18T07:00:08Z</when>", "");
        let err = parse_tracks(unpaired.as_bytes()).expect_err("unpaired");
        assert!(err.to_string().contains("1 times for 2 positions"), "{err}");
        let bad_time = ONE.replace("2024-10-18T07:00:08Z", "yesterday");
        assert!(parse_tracks(bad_time.as_bytes()).is_err());
        let bad_coord = ONE.replace("190.5,", "north,");
        assert!(parse_tracks(bad_coord.as_bytes()).is_err());
        let off_globe = ONE.replace("35.90196,28", "95.0,28");
        assert!(parse_tracks(off_globe.as_bytes()).is_err());
        let cut = &ONE[..ONE.find("</Placemark>").expect("a placemark")];
        let err = parse_tracks(cut.as_bytes()).expect_err("cut short");
        assert!(err.to_string().contains("byte"), "{err}");
        assert!(parse_tracks(b"\xff\xfe").is_err());
    }
}
