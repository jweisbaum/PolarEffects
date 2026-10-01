//! Tracks for the map (spec.md 9.1), as one packed binary buffer.
//!
//! Fifty tracks of ten thousand fixes are half a million positions: as JSON
//! that is tens of megabytes to parse on every edit, so, like the 3D scene
//! (`polar3d.rs`), the positions travel as packed little-endian `f32` and
//! `u32` the frontend views in place.
//!
//! # Wire layout, version 1
//!
//! The Rust side of the layout; the frontend's mirror is
//! `ui/src/map/trackPacket.ts`, and both are held to the same bytes by
//! `ui/src/map/fixtures/tracks-v1.bin`.
//!
//! ```text
//! header, 4 × u32 (16 bytes)
//!   0  magic    0x544d4550 (the bytes "PEMT")
//!   1  version  1
//!   2  T        tracks
//!   3  F        fixes, all tracks together
//! tracks, T × 5 u32
//!   id_lo, id_hi, colour 0x00RRGGBB, first (index of its first fix), count
//! fixes (structure of arrays, tracks one after another)
//!   f32 [F × 2]  longitude, latitude, degrees. Longitudes are **unwrapped**
//!                along each track (each within 180° of the one before), so
//!                a track crossing the antimeridian is one continuous line;
//!                the first fix of each track is in [-180, 180).
//!   u32 [F × 2]  sample id, lo then hi
//!   u32 [F]      flags: bit 0 excluded by hand, bit 1 filtered out
//! ```
//!
//! Only visible track sources are sent (a hidden source leaves every plot,
//! D15), in list order.

use pe_core::Project;
use pe_core::source::Source;
use pe_core::track::Track;

use crate::commands::AppState;
use crate::error::Result;

/// "PEMT" read as a little-endian u32.
pub const TRACKS_MAGIC: u32 = u32::from_le_bytes(*b"PEMT");
/// The wire layout's version.
pub const TRACKS_VERSION: u32 = 1;
/// Header length in bytes.
pub const HEADER_BYTES: usize = 16;
/// Flag: excluded by hand.
pub const FLAG_EXCLUDED: u32 = 1;
/// Flag: removed by the filters.
pub const FLAG_FILTERED: u32 = 2;

/// One track as the map draws it.
#[derive(Debug, Clone, PartialEq)]
pub struct MapTrack {
    /// The source's id.
    pub id: u64,
    /// `0x00RRGGBB`.
    pub colour: u32,
    /// (unwrapped longitude, latitude) per fix.
    pub points: Vec<(f32, f32)>,
    /// Sample id per fix.
    pub samples: Vec<u64>,
    /// Flags per fix.
    pub flags: Vec<u32>,
}

/// Longitudes made continuous along a track: each differs from the one
/// before by at most 180°, so the line never jumps across the map.
pub fn unwrap_longitudes(lons: impl IntoIterator<Item = f64>) -> Vec<f64> {
    let mut out: Vec<f64> = Vec::new();
    for lon in lons {
        let next = match out.last() {
            None => lon,
            Some(&prev) => {
                let step = (lon - prev).rem_euclid(360.0);
                prev + if step > 180.0 { step - 360.0 } else { step }
            }
        };
        out.push(next);
    }
    out
}

fn track_of(source: &Source, track: &Track, filtered: &[bool]) -> MapTrack {
    let excluded = &source.overlay.excluded_samples;
    let lons = unwrap_longitudes(track.samples.iter().map(|s| s.lon));
    MapTrack {
        id: source.id.raw(),
        colour: u32::from_str_radix(source.colour.as_str().trim_start_matches('#'), 16)
            .unwrap_or(0),
        points: track
            .samples
            .iter()
            .zip(&lons)
            .map(|(s, lon)| (*lon as f32, s.lat as f32))
            .collect(),
        samples: track.samples.iter().map(|s| s.id.raw()).collect(),
        flags: track
            .samples
            .iter()
            .zip(filtered)
            .map(|(s, out)| {
                let mut flags = 0;
                if excluded.binary_search(&s.id).is_ok() {
                    flags |= FLAG_EXCLUDED;
                }
                if *out {
                    flags |= FLAG_FILTERED;
                }
                flags
            })
            .collect(),
    }
}

/// Every visible track of a project, as the map draws them.
pub fn tracks_of(project: &Project) -> Vec<MapTrack> {
    let flags = crate::priority::filter_flags(project);
    project
        .sources
        .iter()
        .filter(|s| s.visible)
        .filter_map(|s| {
            s.track().map(|t| {
                track_of(
                    s,
                    t,
                    flags
                        .get(&s.id.raw())
                        .map(Vec::as_slice)
                        .unwrap_or_default(),
                )
            })
        })
        .collect()
}

/// Packs tracks into the wire layout in the module documentation.
pub fn pack(tracks: &[MapTrack]) -> Vec<u8> {
    let fixes: usize = tracks.iter().map(|t| t.points.len()).sum();
    let mut out = Vec::with_capacity(HEADER_BYTES + tracks.len() * 20 + fixes * 20);
    let u = |out: &mut Vec<u8>, value: u32| out.extend_from_slice(&value.to_le_bytes());
    u(&mut out, TRACKS_MAGIC);
    u(&mut out, TRACKS_VERSION);
    u(&mut out, tracks.len() as u32);
    u(&mut out, fixes as u32);
    let mut first = 0u32;
    for track in tracks {
        u(&mut out, track.id as u32);
        u(&mut out, (track.id >> 32) as u32);
        u(&mut out, track.colour);
        u(&mut out, first);
        u(&mut out, track.points.len() as u32);
        first += track.points.len() as u32;
    }
    for track in tracks {
        for (lon, lat) in &track.points {
            out.extend_from_slice(&lon.to_le_bytes());
            out.extend_from_slice(&lat.to_le_bytes());
        }
    }
    for track in tracks {
        for id in &track.samples {
            u(&mut out, *id as u32);
            u(&mut out, (*id >> 32) as u32);
        }
    }
    for track in tracks {
        for flags in &track.flags {
            u(&mut out, *flags);
        }
    }
    out
}

/// The open project's tracks for the map, packed (layout in the module
/// documentation).
#[tauri::command]
pub fn map_tracks(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
) -> Result<tauri::ipc::Response> {
    let state = state.scoped(boat_context);
    tracks_bytes(&state).map(tauri::ipc::Response::new)
}

/// [`map_tracks`] without a Tauri handle.
pub fn tracks_bytes(state: &AppState) -> Result<Vec<u8>> {
    state.with_session(|session| {
        let open = session.require_open()?;
        Ok(pack(&tracks_of(&open.project)))
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn fixture() -> Vec<MapTrack> {
        vec![
            MapTrack {
                id: 4,
                colour: 0x4e79a7,
                points: vec![(179.5, 10.0), (180.5, 10.25)],
                samples: vec![5, (2 << 32) | 6],
                flags: vec![0, FLAG_FILTERED],
            },
            MapTrack {
                id: (1 << 32) | 9,
                colour: 0xe15759,
                points: vec![(-1.5, 50.0)],
                samples: vec![11],
                flags: vec![FLAG_EXCLUDED | FLAG_FILTERED],
            },
        ]
    }

    fn word(bytes: &[u8], index: usize) -> u32 {
        u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    }

    /// Offsets computed by hand from the layout.
    #[test]
    fn the_layout_is_the_documented_one() {
        let bytes = pack(&fixture());
        // 4 header + 2×5 tracks + 3×2 points + 3×2 ids + 3 flags = 29 words.
        assert_eq!(bytes.len(), 29 * 4);
        assert_eq!(&bytes[0..4], b"PEMT");
        assert_eq!(
            (1..4).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [1, 2, 3]
        );
        assert_eq!(
            (4..14).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [4, 0, 0x4e79a7, 0, 2, 9, 1, 0xe15759, 2, 1]
        );
        let float = |i| f32::from_bits(word(&bytes, i));
        assert_eq!(
            (14..20).map(float).collect::<Vec<_>>(),
            [179.5, 10.0, 180.5, 10.25, -1.5, 50.0]
        );
        assert_eq!(
            (20..26).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [5, 0, 6, 2, 11, 0]
        );
        assert_eq!(
            (26..29).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [0, FLAG_FILTERED, FLAG_EXCLUDED | FLAG_FILTERED]
        );
    }

    /// The bytes the frontend's unpacking test reads. `PE_BLESS=1` rewrites
    /// the file after a deliberate layout change.
    #[test]
    fn the_frontend_fixture_holds_these_bytes() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../ui/src/map/fixtures/tracks-v1.bin");
        let bytes = pack(&fixture());
        if std::env::var_os("PE_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    /// Across the antimeridian both ways, and round the world: each step
    /// is the short way, so the line stays continuous.
    #[test]
    fn longitudes_unwrap_across_the_antimeridian() {
        assert_eq!(
            unwrap_longitudes([179.0, -179.0, -178.0]),
            [179.0, 181.0, 182.0]
        );
        assert_eq!(unwrap_longitudes([-179.0, 179.0]), [-179.0, -181.0]);
        let round: Vec<f64> = (0..=8).map(|k| f64::from(k * 45) - 180.0).collect();
        let unwrapped = unwrap_longitudes(round.iter().copied().chain([-135.0]));
        assert_eq!(unwrapped.last(), Some(&225.0));
        assert!(unwrap_longitudes(std::iter::empty()).is_empty());
    }
}
