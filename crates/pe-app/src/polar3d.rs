//! The 3D polar view over IPC (spec.md 10.1–10.3).
//!
//! Rust assembles everything the scene draws — every visible polar source's
//! grid nodes and surface, every sample dot, which of them are excluded —
//! and sends it as one binary buffer rather than JSON: 200,000 dots as JSON
//! would be tens of megabytes of text to parse, where packed `f32` is copied
//! straight into the GPU buffers (plan.md M7).
//!
//! # Wire layout, version 1
//!
//! The one place the layout is defined on the Rust side; the frontend's
//! mirror is `ui/src/polar/scenePacket.ts`, and both are held to the same
//! bytes by `ui/src/polar/fixtures/scene-v1.bin`. Every value is
//! little-endian and 4 bytes wide except the time origin, and every section
//! starts on a 4-byte boundary, so the frontend views each array in place.
//!
//! ```text
//! header, 8 × u32 (32 bytes)
//!   0  magic       0x44334550 (the bytes "PE3D")
//!   1  version     1
//!   2  S           sources
//!   3  N           polar nodes
//!   4  M           samples
//!   5  F           surfaces
//!   6–7 time_origin i64, UTC epoch seconds: sample times are relative to it
//! sources, S × 4 u32
//!   id_lo, id_hi, colour 0x00RRGGBB, kind (0 ORC, 1 polar file, 2 track)
//! nodes (structure of arrays)
//!   f32 [N × 3]  TWA °, TWS kn, BSP kn
//!   u32 [N]      source, an index into the sources section
//!   u32 [N]      cell: TWA index | TWS index << 16, in the source's own grid
//!   u32 [N]      flags
//! samples (structure of arrays)
//!   f32 [M × 3]  TWA °, TWS kn, BSP kn
//!   u32 [M]      source index
//!   u32 [M × 2]  sample id, lo then hi
//!   f32 [M]      Hs, metres (NaN: none)
//!   f32 [M]      current speed, knots (NaN: none)
//!   f32 [M]      time, seconds since time_origin
//!   u32 [M]      flags
//! surfaces, F times
//!   u32 source index (0xFFFFFFFF: the blend), u32 ni, u32 nj
//!   f32 [ni] TWA axis, f32 [nj] TWS axis
//!   f32 [ni × nj] BSP, TWA-major (i × nj + j), NaN for an empty cell
//! ```
//!
//! Flags: bit 0 excluded (spec.md 10.3), bit 1 filtered out (spec.md 7.6).
//!
//! A sample is sent only once it has a place in the polar, which needs its
//! wind (M9): a track without environment adds no dot rather than one at an
//! invented position. The blend surface is never sent until the blend
//! exists (M14); the layout already carries it.

use std::collections::{BTreeMap, BTreeSet};

use pe_core::command::{
    EXCLUDE_DOTS_LABEL, EXCLUDE_NODES_LABEL, EXCLUDE_SAMPLES_LABEL, INCLUDE_DOTS_LABEL,
    INCLUDE_NODES_LABEL, INCLUDE_SAMPLES_LABEL,
};
use pe_core::source::{CellRef, Source, SourceKind};
use pe_core::{Command, Project, SampleId, SourceId};
use serde::Deserialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::edit;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// "PE3D" read as a little-endian u32.
pub const SCENE_MAGIC: u32 = u32::from_le_bytes(*b"PE3D");
/// The wire layout's version.
pub const SCENE_VERSION: u32 = 1;
/// Header length in bytes.
pub const HEADER_BYTES: usize = 32;
/// The source index a blend surface carries.
pub const BLEND_SOURCE: u32 = u32::MAX;
/// Flag: excluded from the blend.
pub const FLAG_EXCLUDED: u32 = 1;
/// Flag: removed by the source's sample filters.
pub const FLAG_FILTERED: u32 = 2;

/// Kind codes in the sources section.
pub const KIND_ORC: u32 = 0;
/// A polar file.
pub const KIND_POLAR_FILE: u32 = 1;
/// A track.
pub const KIND_TRACK: u32 = 2;

/// One source the scene refers to.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneSource {
    /// Its id.
    pub id: u64,
    /// `0x00RRGGBB`.
    pub colour: u32,
    /// [`KIND_ORC`], [`KIND_POLAR_FILE`] or [`KIND_TRACK`].
    pub kind: u32,
}

/// One grid node of a polar source.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneNode {
    /// TWA, degrees.
    pub twa: f32,
    /// TWS, knots.
    pub tws: f32,
    /// BSP, knots.
    pub bsp: f32,
    /// Index into [`Scene::sources`].
    pub source: u32,
    /// Row in the source's grid.
    pub twa_index: u16,
    /// Column in the source's grid.
    pub tws_index: u16,
    /// Excluded from the blend.
    pub excluded: bool,
}

/// One track sample.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneSample {
    /// TWA, degrees, folded.
    pub twa: f32,
    /// TWS, knots.
    pub tws: f32,
    /// BSP, knots.
    pub bsp: f32,
    /// Index into [`Scene::sources`].
    pub source: u32,
    /// The sample's id.
    pub id: u64,
    /// Significant wave height, metres; NaN when unknown.
    pub hs: f32,
    /// Current speed, knots; NaN when unknown.
    pub current: f32,
    /// Seconds since [`Scene::time_origin`].
    pub time: f32,
    /// Excluded by hand.
    pub excluded: bool,
    /// Removed by the sample filters.
    pub filtered: bool,
}

/// One surface: a polar source's grid over its own axes.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneSurface {
    /// Index into [`Scene::sources`], or [`BLEND_SOURCE`].
    pub source: u32,
    /// TWA axis, degrees.
    pub twa: Vec<f32>,
    /// TWS axis, knots.
    pub tws: Vec<f32>,
    /// BSP, TWA-major; NaN for an empty cell.
    pub bsp: Vec<f32>,
}

/// Everything the 3D view draws.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Scene {
    /// UTC epoch seconds that sample times count from.
    pub time_origin: i64,
    /// Sources referred to, in list order.
    pub sources: Vec<SceneSource>,
    /// Polar nodes.
    pub nodes: Vec<SceneNode>,
    /// Samples.
    pub samples: Vec<SceneSample>,
    /// Surfaces.
    pub surfaces: Vec<SceneSurface>,
}

fn kind_code(kind: &SourceKind) -> u32 {
    match kind {
        SourceKind::Orc { .. } => KIND_ORC,
        SourceKind::PolarFile { .. } => KIND_POLAR_FILE,
        SourceKind::Track { .. } => KIND_TRACK,
    }
}

fn colour_code(source: &Source) -> u32 {
    u32::from_str_radix(source.colour.as_str().trim_start_matches('#'), 16).unwrap_or(0)
}

/// The scene for a project: every visible source (hidden ones leave every
/// plot, D15), a node per grid cell with a value and a surface per polar
/// source, both over the source's own axes — nothing is resampled or
/// extrapolated, and an empty cell is a hole.
pub fn scene_of(project: &Project) -> Scene {
    let mut scene = Scene::default();
    let mut times: Vec<i64> = Vec::new();
    for source in project.sources.iter().filter(|source| source.visible) {
        let index = scene.sources.len() as u32;
        scene.sources.push(SceneSource {
            id: source.id.raw(),
            colour: colour_code(source),
            kind: kind_code(&source.kind),
        });
        if let Some(track) = source.track() {
            let use_corrected = project.blend.use_corrected;
            let filtered = pe_tracks::filtered_out(track, &source.overlay.filters, use_corrected);
            let excluded = &source.overlay.excluded_samples;
            for (sample, filtered) in track.samples.iter().zip(filtered) {
                let Some((twa, tws, bsp)) = pe_tracks::polar_point(sample, use_corrected) else {
                    continue;
                };
                scene.samples.push(SceneSample {
                    twa: twa as f32,
                    tws: tws as f32,
                    bsp: bsp as f32,
                    source: index,
                    id: sample.id.raw(),
                    hs: sample.hs_m.map_or(f32::NAN, |v| v as f32),
                    current: sample.current_speed.map_or(f32::NAN, |v| v as f32),
                    // Made relative to the time origin below.
                    time: 0.0,
                    excluded: excluded.binary_search(&sample.id).is_ok(),
                    filtered,
                });
                times.push(sample.t);
            }
            continue;
        }
        let Some(grid) = pe_polar::source_polar(source) else {
            continue;
        };
        let (ni, nj) = (grid.twa.len(), grid.tws.len());
        let mut bsp = vec![f32::NAN; ni * nj];
        for (i, twa) in grid.twa.iter().enumerate() {
            for (j, tws) in grid.tws.iter().enumerate() {
                let Some(value) = grid.get(i, j) else {
                    continue;
                };
                bsp[i * nj + j] = value as f32;
                // Axes are at most 512 long (pe-polar's MAX_AXIS_VALUES), so
                // an index always fits; a grid that broke that would lose
                // its node, not the scene.
                let (Ok(twa_index), Ok(tws_index)) = (u16::try_from(i), u16::try_from(j)) else {
                    continue;
                };
                scene.nodes.push(SceneNode {
                    twa: *twa as f32,
                    tws: *tws as f32,
                    bsp: value as f32,
                    source: index,
                    twa_index,
                    tws_index,
                    excluded: source.overlay.is_cell_excluded(*twa, *tws),
                });
            }
        }
        scene.surfaces.push(SceneSurface {
            source: index,
            twa: grid.twa.iter().map(|v| *v as f32).collect(),
            tws: grid.tws.iter().map(|v| *v as f32).collect(),
            bsp,
        });
    }
    // Sample times travel as f32 seconds from the earliest, which keeps
    // them to the second over any race.
    scene.time_origin = times.iter().copied().min().unwrap_or(0);
    for (sample, t) in scene.samples.iter_mut().zip(&times) {
        sample.time = (t - scene.time_origin) as f32;
    }
    // The blend surface joins here once it exists (M14): it is derived, and
    // nothing in this module invents one (invariant 2).
    scene
}

fn flags(excluded: bool, filtered: bool) -> u32 {
    (if excluded { FLAG_EXCLUDED } else { 0 }) | (if filtered { FLAG_FILTERED } else { 0 })
}

/// Packs a scene into the wire layout in the module documentation.
pub fn pack(scene: &Scene) -> Vec<u8> {
    let (n, m) = (scene.nodes.len(), scene.samples.len());
    let surface_words: usize = scene
        .surfaces
        .iter()
        .map(|s| 3 + s.twa.len() + s.tws.len() + s.bsp.len())
        .sum();
    let words = 8 + scene.sources.len() * 4 + n * 6 + m * 10 + surface_words;
    let mut out = Vec::with_capacity(words * 4);
    let u = |out: &mut Vec<u8>, value: u32| out.extend_from_slice(&value.to_le_bytes());
    let f = |out: &mut Vec<u8>, value: f32| out.extend_from_slice(&value.to_le_bytes());

    u(&mut out, SCENE_MAGIC);
    u(&mut out, SCENE_VERSION);
    u(&mut out, scene.sources.len() as u32);
    u(&mut out, n as u32);
    u(&mut out, m as u32);
    u(&mut out, scene.surfaces.len() as u32);
    out.extend_from_slice(&scene.time_origin.to_le_bytes());

    for source in &scene.sources {
        u(&mut out, source.id as u32);
        u(&mut out, (source.id >> 32) as u32);
        u(&mut out, source.colour);
        u(&mut out, source.kind);
    }

    for node in &scene.nodes {
        f(&mut out, node.twa);
        f(&mut out, node.tws);
        f(&mut out, node.bsp);
    }
    for node in &scene.nodes {
        u(&mut out, node.source);
    }
    for node in &scene.nodes {
        u(
            &mut out,
            u32::from(node.twa_index) | (u32::from(node.tws_index) << 16),
        );
    }
    for node in &scene.nodes {
        u(&mut out, flags(node.excluded, false));
    }

    for sample in &scene.samples {
        f(&mut out, sample.twa);
        f(&mut out, sample.tws);
        f(&mut out, sample.bsp);
    }
    for sample in &scene.samples {
        u(&mut out, sample.source);
    }
    for sample in &scene.samples {
        u(&mut out, sample.id as u32);
        u(&mut out, (sample.id >> 32) as u32);
    }
    for sample in &scene.samples {
        f(&mut out, sample.hs);
    }
    for sample in &scene.samples {
        f(&mut out, sample.current);
    }
    for sample in &scene.samples {
        f(&mut out, sample.time);
    }
    for sample in &scene.samples {
        u(&mut out, flags(sample.excluded, sample.filtered));
    }

    for surface in &scene.surfaces {
        u(&mut out, surface.source);
        u(&mut out, surface.twa.len() as u32);
        u(&mut out, surface.tws.len() as u32);
        for value in surface.twa.iter().chain(&surface.tws).chain(&surface.bsp) {
            f(&mut out, *value);
        }
    }
    out
}

/// The 3D view's scene for the open project, packed (see the module
/// documentation for the layout).
#[tauri::command]
pub fn polar_scene(state: tauri::State<'_, AppState>) -> Result<tauri::ipc::Response> {
    scene_bytes(&state).map(tauri::ipc::Response::new)
}

/// [`polar_scene`] without a Tauri handle.
pub fn scene_bytes(state: &AppState) -> Result<Vec<u8>> {
    state.with_session(|session| {
        let open = session.require_open()?;
        Ok(pack(&scene_of(&open.project)))
    })
}

/// A polar node, as the scene named it: a source and the node's place in
/// that source's own grid. Indices rather than axis values, so nothing is
/// lost to the `f32` the scene travels in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Deserialize, TS)]
#[ts(export_to = "PolarNodeRef.ts")]
pub struct PolarNodeRef {
    /// The source's id.
    pub source_id: u64,
    /// Row in the source's TWA axis.
    pub twa_index: u32,
    /// Column in the source's TWS axis.
    pub tws_index: u32,
}

/// Excludes a selection from the blend, or includes it again (spec.md 10.3),
/// as one undoable change. `nodes` are polar nodes of ORC and file sources;
/// `samples` are track sample ids. Dots already in the asked state are left
/// alone; a selection that changes nothing records nothing.
#[tauri::command]
pub fn set_excluded(
    state: tauri::State<'_, AppState>,
    nodes: Vec<PolarNodeRef>,
    samples: Vec<u64>,
    excluded: bool,
) -> Result<ProjectSummary> {
    excluded_set(&state, &nodes, &samples, excluded)
}

/// [`set_excluded`] without a Tauri handle.
pub fn excluded_set(
    state: &AppState,
    nodes: &[PolarNodeRef],
    samples: &[u64],
    excluded: bool,
) -> Result<ProjectSummary> {
    edit::apply(state, |project| {
        let mut commands = node_commands(project, nodes, excluded)?;
        let node_count = commands.len();
        commands.extend(sample_commands(project, samples, excluded)?);
        let label = match (node_count > 0, commands.len() > node_count, excluded) {
            (true, true, true) => EXCLUDE_DOTS_LABEL,
            (true, true, false) => INCLUDE_DOTS_LABEL,
            (true, false, true) => EXCLUDE_NODES_LABEL,
            (true, false, false) => INCLUDE_NODES_LABEL,
            (false, _, true) => EXCLUDE_SAMPLES_LABEL,
            (false, _, false) => INCLUDE_SAMPLES_LABEL,
        };
        Ok(match commands.len() {
            0 => None,
            1 => commands.pop(),
            _ => Some(Command::Batch {
                label: label.to_owned(),
                commands,
            }),
        })
    })
}

/// One command per track source for the samples not yet in the asked
/// state. Every id must be a sample of some track in the project.
fn sample_commands(project: &Project, samples: &[u64], excluded: bool) -> Result<Vec<Command>> {
    if samples.is_empty() {
        return Ok(Vec::new());
    }
    let mut wanted: Vec<SampleId> = samples.iter().map(|id| SampleId(*id)).collect();
    wanted.sort_unstable();
    wanted.dedup();
    let mut found = 0;
    let mut commands = Vec::new();
    for source in &project.sources {
        let Some(track) = source.track() else {
            continue;
        };
        let list = &source.overlay.excluded_samples;
        let mut changed: Vec<SampleId> = track
            .samples
            .iter()
            .map(|s| s.id)
            .filter(|id| wanted.binary_search(id).is_ok())
            .inspect(|_| found += 1)
            .filter(|id| list.binary_search(id).is_ok() != excluded)
            .collect();
        if changed.is_empty() {
            continue;
        }
        changed.sort_unstable();
        let source = source.id;
        commands.push(if excluded {
            Command::ExcludeSamples {
                source,
                samples: changed,
            }
        } else {
            Command::IncludeSamples {
                source,
                samples: changed,
            }
        });
    }
    if found != wanted.len() {
        return Err(AppError::BadOption {
            field: "Sample",
            value: format!(
                "{} of the selected samples are not in the project",
                wanted.len() - found
            ),
        });
    }
    Ok(commands)
}

fn bad_node(node: &PolarNodeRef) -> AppError {
    AppError::BadOption {
        field: "Polar node",
        value: format!(
            "source {} row {} column {}",
            node.source_id, node.twa_index, node.tws_index
        ),
    }
}

/// The commands that put every node in `nodes` into the asked state: one
/// per source (the caller batches them, so the action is one entry).
fn node_commands(
    project: &Project,
    nodes: &[PolarNodeRef],
    excluded: bool,
) -> Result<Vec<Command>> {
    let mut by_source: BTreeMap<u64, BTreeSet<(u32, u32)>> = BTreeMap::new();
    for node in nodes {
        by_source
            .entry(node.source_id)
            .or_default()
            .insert((node.twa_index, node.tws_index));
    }
    let mut commands = Vec::new();
    for (id, cells) in by_source {
        let source = project
            .source(SourceId(id))
            .ok_or(AppError::Core(pe_core::CoreError::MissingSource(id)))?;
        let grid = pe_polar::source_polar(source).ok_or(AppError::BadOption {
            field: "Polar node",
            value: format!("{} is a track", source.label),
        })?;
        let mut changed = Vec::new();
        for (i, j) in cells {
            let node = PolarNodeRef {
                source_id: id,
                twa_index: i,
                tws_index: j,
            };
            let (i, j) = (i as usize, j as usize);
            let (Some(twa), Some(tws), Some(_)) =
                (grid.twa.get(i), grid.tws.get(j), grid.get(i, j))
            else {
                return Err(bad_node(&node));
            };
            if source.overlay.is_cell_excluded(*twa, *tws) != excluded {
                changed.push(CellRef {
                    twa: *twa,
                    tws: *tws,
                });
            }
        }
        if changed.is_empty() {
            continue;
        }
        let source = SourceId(id);
        commands.push(if excluded {
            Command::ExcludeCells {
                source,
                cells: changed,
            }
        } else {
            Command::IncludeCells {
                source,
                cells: changed,
            }
        });
    }
    Ok(commands)
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    /// A small scene with one of everything, including a sample and a blend
    /// surface the project cannot make yet, so the whole layout is pinned.
    fn fixture_scene() -> Scene {
        Scene {
            time_origin: 1_753_000_000,
            sources: vec![
                SceneSource {
                    id: 7,
                    colour: 0x4e79a7,
                    kind: KIND_ORC,
                },
                SceneSource {
                    id: (3 << 32) | 9,
                    colour: 0xe15759,
                    kind: KIND_TRACK,
                },
            ],
            nodes: vec![
                SceneNode {
                    twa: 52.0,
                    tws: 6.0,
                    bsp: 5.9,
                    source: 0,
                    twa_index: 1,
                    tws_index: 0,
                    excluded: false,
                },
                SceneNode {
                    twa: 90.0,
                    tws: 12.0,
                    bsp: 8.4,
                    source: 0,
                    twa_index: 2,
                    tws_index: 1,
                    excluded: true,
                },
            ],
            samples: vec![SceneSample {
                twa: 135.0,
                tws: 14.25,
                bsp: 9.5,
                source: 1,
                id: (1 << 32) | 5,
                hs: 1.5,
                current: f32::NAN,
                time: 600.0,
                excluded: true,
                filtered: true,
            }],
            surfaces: vec![
                SceneSurface {
                    source: 0,
                    twa: vec![52.0, 90.0],
                    tws: vec![6.0, 12.0],
                    bsp: vec![5.9, 7.3, 6.8, f32::NAN],
                },
                SceneSurface {
                    source: BLEND_SOURCE,
                    twa: vec![45.0],
                    tws: vec![10.0],
                    bsp: vec![6.0],
                },
            ],
        }
    }

    fn word(bytes: &[u8], index: usize) -> u32 {
        u32::from_le_bytes(bytes[index * 4..index * 4 + 4].try_into().unwrap())
    }

    fn float(bytes: &[u8], index: usize) -> f32 {
        f32::from_bits(word(bytes, index))
    }

    /// Offsets computed by hand from the layout, not from `pack`.
    #[test]
    fn the_layout_is_the_documented_one() {
        let bytes = pack(&fixture_scene());
        // 8 header + 2×4 sources + 2×6 nodes + 1×10 samples
        // + (3 + 2 + 2 + 4) + (3 + 1 + 1 + 1) surfaces = 55 words.
        assert_eq!(bytes.len(), 55 * 4);
        assert_eq!(&bytes[0..4], b"PE3D");
        assert_eq!(
            (1..6).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [1, 2, 2, 1, 2]
        );
        assert_eq!(
            i64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            1_753_000_000
        );
        // Sources: words 8–15.
        assert_eq!(
            (8..16).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [7, 0, 0x4e79a7, 0, 9, 3, 0xe15759, 2]
        );
        // Node positions: words 16–21, then source 22–23, cell 24–25, flags 26–27.
        assert_eq!(
            (16..22).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [52.0, 6.0, 5.9, 90.0, 12.0, 8.4]
        );
        assert_eq!(
            (22..28).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [0, 0, 1, 2 | (1 << 16), 0, FLAG_EXCLUDED]
        );
        // Sample: position 28–30, source 31, id 32–33, hs 34, current 35,
        // time 36, flags 37.
        assert_eq!(
            (28..31).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [135.0, 14.25, 9.5]
        );
        assert_eq!(
            (31..34).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [1, 5, 1]
        );
        assert_eq!(float(&bytes, 34), 1.5);
        assert!(float(&bytes, 35).is_nan());
        assert_eq!(float(&bytes, 36), 600.0);
        assert_eq!(word(&bytes, 37), FLAG_EXCLUDED | FLAG_FILTERED);
        // First surface: header 38–40, axes 41–44, values 45–48.
        assert_eq!(
            (38..41).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [0, 2, 2]
        );
        assert_eq!(
            (41..48).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [52.0, 90.0, 6.0, 12.0, 5.9, 7.3, 6.8]
        );
        assert!(float(&bytes, 48).is_nan());
        // The blend: 49–51, then its one-cell grid.
        assert_eq!(
            (49..52).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [u32::MAX, 1, 1]
        );
        assert_eq!(
            (52..55).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [45.0, 10.0, 6.0]
        );
    }

    /// The same bytes the frontend's unpacking test reads, so the two sides
    /// cannot drift apart. `PE_BLESS=1` rewrites the file after a deliberate
    /// layout change (and the version must change with it).
    #[test]
    fn the_frontend_fixture_holds_these_bytes() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../ui/src/polar/fixtures/scene-v1.bin");
        let bytes = pack(&fixture_scene());
        if std::env::var_os("PE_BLESS").is_some() {
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, &bytes).unwrap();
        }
        assert_eq!(std::fs::read(&path).unwrap(), bytes);
    }

    #[test]
    fn an_empty_scene_is_a_header_alone() {
        let bytes = pack(&Scene::default());
        assert_eq!(bytes.len(), HEADER_BYTES);
        assert_eq!(word(&bytes, 0), SCENE_MAGIC);
    }

    /// The spec.md 13 scale: 200,000 samples pack into 8 MB with no
    /// per-sample allocation.
    #[test]
    fn two_hundred_thousand_samples_pack_to_the_expected_size() {
        let sample = fixture_scene().samples[0].clone();
        let scene = Scene {
            samples: vec![sample; 200_000],
            ..Scene::default()
        };
        let started = std::time::Instant::now();
        let bytes = pack(&scene);
        let elapsed = started.elapsed();
        assert_eq!(bytes.len(), HEADER_BYTES + 200_000 * 10 * 4);
        println!("packed 200k samples in {elapsed:?}");
    }
}
