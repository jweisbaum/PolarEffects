//! The 3D polar view over IPC (spec.md 10.1–10.3).
//!
//! Rust assembles everything the scene draws — every visible polar source's
//! grid nodes and surface, every sample dot, which of them are excluded —
//! and sends it as one binary buffer rather than JSON: 200,000 dots as JSON
//! would be tens of megabytes of text to parse, where packed `f32` is copied
//! straight into the GPU buffers (plan.md M7).
//!
//! # Wire layout, version 4
//!
//! The one place the layout is defined on the Rust side; the frontend's
//! mirror is `ui/src/polar/scenePacket.ts`, and both are held to the same
//! bytes by `ui/src/polar/fixtures/scene-v4.bin`. Every value is
//! little-endian and 4 bytes wide except the time origin and the samples
//! key, and every section starts on a 4-byte boundary, so the frontend views
//! each array in place.
//!
//! ```text
//! header, 12 × u32 (48 bytes)
//!   0  magic       0x44334550 (the bytes "PE3D")
//!   1  version     4
//!   2  S           sources
//!   3  N           polar nodes
//!   4  M           samples
//!   5  F           surfaces
//!   6–7 time_origin i64, UTC epoch seconds: sample times are relative to it
//!   8–9 samples_key u64: names the samples section (below)
//!   10 samples_mode 0 full, 1 flags only
//!   11 reserved, 0
//! sources, S × 4 u32
//!   id_lo, id_hi, colour 0x00RRGGBB, kind (0 ORC, 1 polar file, 2 track, 3 ORR)
//! nodes (structure of arrays)
//!   f32 [N × 3]  TWA °, TWS kn, BSP kn
//!   u32 [N]      source, an index into the sources section
//!   u32 [N]      cell: TWA index | TWS index << 16, in the source's own grid
//!   u32 [N]      flags
//! samples, full (structure of arrays)
//!   f32 [M × 3]  TWA °, TWS kn, BSP kn
//!   u32 [M]      source index
//!   u32 [M × 2]  sample id, lo then hi
//!   f32 [M]      Hs, metres (NaN: none)
//!   f32 [M]      current speed, knots (NaN: none)
//!   f32 [M]      time, seconds since time_origin
//!   f32 [M]      wave period, seconds (NaN: none)
//!   f32 [M]      wave angle off the bow, degrees (NaN: none)
//!   f32 [M]      wave angle to wind, degrees (NaN: none)
//!   u32 [M]      flags
//! samples, flags only
//!   u32 [M]      flags
//! surfaces, F times
//!   u32 source index (0xFFFFFFFF: the blend), u32 ni, u32 nj
//!   f32 [ni] TWA axis, f32 [nj] TWS axis
//!   f32 [ni × nj] BSP, TWA-major (i × nj + j), NaN for an empty cell
//! ```
//!
//! Flags: bit 0 excluded (spec.md 10.3), bit 1 filtered out (spec.md 7.6),
//! bit 2 edited (a node whose cell holds an override, spec.md 10.4). Bits
//! 8–9 of a sample's flags are its band of the local solar day (spec.md
//! 10.2; `pe_tracks::daytime`): 0 night, 1 morning, 2 afternoon, 3 evening.
//! It rides in the flags so the flags-only scene carries it at no cost.
//!
//! **Flags-only scenes** keep an edit fast at 200,000 samples (spec.md 13,
//! plan.md M13): equal samples keys mean the same visible sources in the
//! same order with every sample where it was, so when the frontend names the
//! key it holds and it is still current, only the samples' flags travel
//! (0.8 MB rather than 8 MB) and the frontend keeps the rest of what it has.
//!
//! Nodes and surfaces are each polar source's grid **as edited** (its
//! overrides written in), over its own axes; excluded nodes stay drawn, as
//! crosses. In edit mode on a track (`focus`), the track's polar segment on
//! the output grid joins them as its editable surface (spec.md 10.4).
//!
//! A sample is sent only once it has a place in the polar, which needs its
//! wind (M9): a track without environment adds no dot rather than one at an
//! invented position. The blend (spec.md 12.3) is one more surface on the
//! output grid, source index [`BLEND_SOURCE`], sent while the Blend entry
//! is shown; the frontend draws it opaque in the Blend entry's colour.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use pe_core::command::{
    EXCLUDE_DOTS_LABEL, EXCLUDE_NODES_LABEL, EXCLUDE_SAMPLES_LABEL, INCLUDE_DOTS_LABEL,
    INCLUDE_NODES_LABEL, INCLUDE_SAMPLES_LABEL,
};
use pe_core::source::{CellRef, Source, SourceKind};
use pe_core::{Command, Project, SampleId, SourceId};
use pe_tracks::daytime::DayBand;
use serde::Deserialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::derived::{Derivations, Derived};
use crate::edit;
use crate::error::{AppError, Result};
use crate::projects::ProjectSummary;

/// "PE3D" read as a little-endian u32.
pub const SCENE_MAGIC: u32 = u32::from_le_bytes(*b"PE3D");
/// The wire layout's version.
pub const SCENE_VERSION: u32 = 4;
/// Header length in bytes.
pub const HEADER_BYTES: usize = 48;
/// The source index a blend surface carries.
pub const BLEND_SOURCE: u32 = u32::MAX;
/// Flag: excluded from the blend.
pub const FLAG_EXCLUDED: u32 = 1;
/// Flag: removed by the source's sample filters.
pub const FLAG_FILTERED: u32 = 2;
/// Flag: a node whose cell holds an edit.
pub const FLAG_EDITED: u32 = 4;
/// Where a sample's flags hold its day band's two-bit code.
pub const FLAG_BAND_SHIFT: u32 = 8;
/// Samples mode: the whole samples section.
pub const SAMPLES_FULL: u32 = 0;
/// Samples mode: only the samples' flags.
pub const SAMPLES_FLAGS_ONLY: u32 = 1;

/// Kind codes in the sources section.
pub const KIND_ORC: u32 = 0;
/// A polar file.
pub const KIND_POLAR_FILE: u32 = 1;
/// A track.
pub const KIND_TRACK: u32 = 2;
/// An ORR certificate variant.
pub const KIND_ORR: u32 = 3;

/// One source the scene refers to.
#[derive(Debug, Clone, PartialEq)]
pub struct SceneSource {
    /// Its id.
    pub id: u64,
    /// `0x00RRGGBB`.
    pub colour: u32,
    /// [`KIND_ORC`], [`KIND_ORR`], [`KIND_POLAR_FILE`] or [`KIND_TRACK`].
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
    /// Holds an edit (spec.md 10.4).
    pub edited: bool,
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
    /// Mean wave period in seconds, NaN when absent.
    pub wave_period: f32,
    /// Wave direction relative to the boat, 0–180 degrees.
    pub wave_angle: f32,
    /// Angle between wave and wind from-directions, 0–180 degrees.
    pub wave_wind_angle: f32,
    /// Current speed, knots; NaN when unknown.
    pub current: f32,
    /// Seconds since [`Scene::time_origin`].
    pub time: f32,
    /// Excluded by hand.
    pub excluded: bool,
    /// Removed by the sample filters.
    pub filtered: bool,
    /// The band of the local solar day it was sailed in.
    pub band: DayBand,
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
    /// Names the samples section (see the module documentation).
    pub samples_key: u64,
    /// Only the samples' flags are meaningful and packed.
    pub flags_only: bool,
}

fn kind_code(kind: &SourceKind) -> u32 {
    match kind {
        SourceKind::Orc { .. } => KIND_ORC,
        SourceKind::Orr { .. } => KIND_ORR,
        SourceKind::PolarFile { .. } => KIND_POLAR_FILE,
        SourceKind::Track { .. } => KIND_TRACK,
    }
}

fn colour_code(source: &Source) -> u32 {
    u32::from_str_radix(source.colour.as_str().trim_start_matches('#'), 16).unwrap_or(0)
}

/// The scene for a project, derived afresh: every visible source (hidden
/// ones leave every plot, D15), a node per grid cell with a value and a
/// surface per polar source, both over the source's own axes as edited —
/// nothing is resampled or extrapolated, and an empty cell is a hole.
pub fn scene_of(project: &Project) -> Scene {
    let mut derivations = Derivations::default();
    let derived = derivations.visible(project);
    let blend = derivations.blend(project);
    let shown = project.blend.visible.then_some(&blend.polar);
    scene_with(project, &derived, shown, None, false)
}

/// The scene from each visible source's derived data, and the blend's
/// surface when it is shown (`blend`). `focus` names the source in edit
/// mode: a track's segment is added as its surface. `flags_only` leaves out
/// everything of the samples but their flags.
pub fn scene_with(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    blend: Option<&pe_polar::Polar>,
    focus: Option<u64>,
    flags_only: bool,
) -> Scene {
    let mut scene = Scene {
        flags_only,
        ..Scene::default()
    };
    let mut times: Vec<i64> = Vec::new();
    for source in project.sources.iter().filter(|source| source.visible) {
        let Some(data) = derived.get(&source.id.raw()) else {
            continue;
        };
        let index = scene.sources.len() as u32;
        scene.sources.push(SceneSource {
            id: source.id.raw(),
            colour: colour_code(source),
            kind: kind_code(&source.kind),
        });
        if let (Some(track), Some(placed)) = (source.track(), data.track.as_ref()) {
            let excluded = &source.overlay.excluded_samples;
            for ((sample, point), filtered) in track
                .samples
                .iter()
                .zip(&placed.points)
                .zip(&placed.filtered)
            {
                let Some((twa, tws, bsp)) = *point else {
                    continue;
                };
                let excluded = excluded.binary_search(&sample.id).is_ok();
                let band = pe_tracks::daytime::day_band(sample.t, sample.lon);
                scene.samples.push(if flags_only {
                    SceneSample {
                        twa: 0.0,
                        tws: 0.0,
                        bsp: 0.0,
                        source: index,
                        id: 0,
                        hs: 0.0,
                        wave_period: 0.0,
                        wave_angle: 0.0,
                        wave_wind_angle: 0.0,
                        current: 0.0,
                        time: 0.0,
                        excluded,
                        filtered: *filtered,
                        band,
                    }
                } else {
                    SceneSample {
                        twa: twa as f32,
                        tws: tws as f32,
                        bsp: bsp as f32,
                        source: index,
                        id: sample.id.raw(),
                        hs: sample.hs_m.map_or(f32::NAN, |v| v as f32),
                        wave_period: sample.wave_period_s.map_or(f32::NAN, |v| v as f32),
                        wave_angle: sample
                            .wave_from
                            .zip(if project.blend.use_corrected {
                                sample.heading_corrected.or(sample.heading)
                            } else {
                                sample.heading
                            })
                            .map_or(f32::NAN, |(w, h)| {
                                pe_tracks::geo::angle_between(w, h) as f32
                            }),
                        wave_wind_angle: sample
                            .wave_from
                            .zip(if project.blend.use_corrected {
                                sample.twd_from_corrected.or(sample.wind_direction())
                            } else {
                                sample.wind_direction()
                            })
                            .map_or(f32::NAN, |(w, wind)| {
                                pe_tracks::geo::angle_between(w, wind) as f32
                            }),
                        current: sample.current_speed.map_or(f32::NAN, |v| v as f32),
                        // Made relative to the time origin below.
                        time: 0.0,
                        excluded,
                        filtered: *filtered,
                        band,
                    }
                });
                times.push(sample.t);
            }
            if focus != Some(source.id.raw()) {
                continue;
            }
        }
        add_grid(
            &mut scene,
            source,
            &data.edited,
            index,
            project.blend.interpolation,
        );
    }
    // Sample times travel as f32 seconds from the earliest, which keeps
    // them to the second over about six months (2^24 s).
    scene.time_origin = times.iter().copied().min().unwrap_or(0);
    if !flags_only {
        for (sample, t) in scene.samples.iter_mut().zip(&times) {
            sample.time = (t - scene.time_origin) as f32;
        }
    }
    if let Some(grid) = blend.filter(|grid| pe_polar::blend::has_value_off_zero_row(grid)) {
        add_surface(
            &mut scene,
            grid,
            BLEND_SOURCE,
            project.blend.interpolation,
            true,
        );
    }
    scene
}

/// A source's grid as nodes and a surface.
fn add_grid(
    scene: &mut Scene,
    source: &Source,
    grid: &pe_polar::Polar,
    index: u32,
    mode: pe_core::project::Interpolation,
) {
    for (i, twa) in grid.twa.iter().enumerate() {
        for (j, tws) in grid.tws.iter().enumerate() {
            let Some(value) = grid.get(i, j) else {
                continue;
            };
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
                edited: source.overlay.override_at(*twa, *tws).is_some(),
            });
        }
    }
    add_surface(scene, grid, index, mode, false);
}

fn add_surface(
    scene: &mut Scene,
    grid: &pe_polar::Polar,
    index: u32,
    mode: pe_core::project::Interpolation,
    blend: bool,
) {
    let refined;
    let grid = if mode == pe_core::project::Interpolation::MonotoneSpline {
        let axis = |values: &[f64]| {
            let mut out = values.to_vec();
            if let (Some(lo), Some(hi)) = (values.first(), values.last()) {
                out.extend((lo.ceil() as u32..=hi.floor() as u32).map(f64::from));
                out.sort_by(f64::total_cmp);
                out.dedup();
            }
            out
        };
        let twa = axis(&grid.twa);
        let tws = axis(&grid.tws);
        refined = if blend {
            pe_polar::blend::resample_blend_mode(grid, &twa, &tws, mode)
        } else {
            let reader = pe_polar::spline::Interpolator::new(grid, mode);
            let bsp = twa
                .iter()
                .map(|a| tws.iter().map(|s| reader.at(*a, *s)).collect())
                .collect();
            pe_polar::Polar { twa, tws, bsp }
        };
        &refined
    } else {
        grid
    };
    scene.surfaces.push(SceneSurface {
        source: index,
        twa: grid.twa.iter().map(|v| *v as f32).collect(),
        tws: grid.tws.iter().map(|v| *v as f32).collect(),
        bsp: grid
            .bsp
            .iter()
            .flatten()
            .map(|v| v.map_or(f32::NAN, |v| v as f32))
            .collect(),
    });
}

fn flags(excluded: bool, filtered: bool, edited: bool) -> u32 {
    (if excluded { FLAG_EXCLUDED } else { 0 })
        | (if filtered { FLAG_FILTERED } else { 0 })
        | (if edited { FLAG_EDITED } else { 0 })
}

fn sample_flags(sample: &SceneSample) -> u32 {
    flags(sample.excluded, sample.filtered, false) | (sample.band.code() << FLAG_BAND_SHIFT)
}

/// Packs a scene into the wire layout in the module documentation.
pub fn pack(scene: &Scene) -> Vec<u8> {
    let (n, m) = (scene.nodes.len(), scene.samples.len());
    let surface_words: usize = scene
        .surfaces
        .iter()
        .map(|s| 3 + s.twa.len() + s.tws.len() + s.bsp.len())
        .sum();
    let per_sample = if scene.flags_only { 1 } else { 13 };
    let words = 12 + scene.sources.len() * 4 + n * 6 + m * per_sample + surface_words;
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
    out.extend_from_slice(&scene.samples_key.to_le_bytes());
    u(
        &mut out,
        if scene.flags_only {
            SAMPLES_FLAGS_ONLY
        } else {
            SAMPLES_FULL
        },
    );
    u(&mut out, 0);

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
        u(&mut out, flags(node.excluded, false, node.edited));
    }

    if scene.flags_only {
        for sample in &scene.samples {
            u(&mut out, sample_flags(sample));
        }
    } else {
        pack_samples(&mut out, scene);
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

/// The full samples section.
fn pack_samples(out: &mut Vec<u8>, scene: &Scene) {
    let mut u = |value: u32| out.extend_from_slice(&value.to_le_bytes());
    let samples = &scene.samples;
    for sample in samples {
        for value in [sample.twa, sample.tws, sample.bsp] {
            u(value.to_bits());
        }
    }
    for sample in samples {
        u(sample.source);
    }
    for sample in samples {
        u(sample.id as u32);
        u((sample.id >> 32) as u32);
    }
    for sample in samples {
        u(sample.hs.to_bits());
    }
    for sample in samples {
        u(sample.current.to_bits());
    }
    for sample in samples {
        u(sample.time.to_bits());
    }
    for sample in samples {
        u(sample.wave_period.to_bits());
    }
    for sample in samples {
        u(sample.wave_angle.to_bits());
    }
    for sample in samples {
        u(sample.wave_wind_angle.to_bits());
    }
    for sample in samples {
        u(sample_flags(sample));
    }
}

/// The 3D view's scene for the open project, packed (see the module
/// documentation for the layout). `focus` is the source in edit mode, if
/// any; `samples_key` the key of the samples the frontend already holds, if
/// any: when it is still current, only their flags are sent.
#[tauri::command]
pub fn polar_scene(
    state: tauri::State<'_, AppState>,
    boat_context: Option<u64>,
    focus: Option<u64>,
    samples_key: Option<u64>,
) -> Result<tauri::ipc::Response> {
    let state = state.scoped(boat_context);
    scene_bytes_for(&state, focus, samples_key).map(tauri::ipc::Response::new)
}

/// The whole scene, without a focus: [`polar_scene`] without a Tauri handle.
pub fn scene_bytes(state: &AppState) -> Result<Vec<u8>> {
    scene_bytes_for(state, None, None)
}

/// [`polar_scene`] without a Tauri handle.
pub fn scene_bytes_for(
    state: &AppState,
    focus: Option<u64>,
    samples_key: Option<u64>,
) -> Result<Vec<u8>> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let key = open.derived.samples_key(&open.project);
        let derived = open.derived.visible(&open.project);
        let blend = open.derived.blend(&open.project);
        let shown = open.project.blend.visible.then_some(&blend.polar);
        let mut scene = scene_with(
            &open.project,
            &derived,
            shown,
            focus,
            samples_key == Some(key),
        );
        scene.samples_key = key;
        Ok(pack(&scene))
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
    boat_context: Option<u64>,
    nodes: Vec<PolarNodeRef>,
    samples: Vec<u64>,
    excluded: bool,
) -> Result<ProjectSummary> {
    let state = state.scoped(boat_context);
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
            samples_key: 0x0010_0304_0506_0708,
            flags_only: false,
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
                    edited: true,
                },
                SceneNode {
                    twa: 90.0,
                    tws: 12.0,
                    bsp: 8.4,
                    source: 0,
                    twa_index: 2,
                    tws_index: 1,
                    excluded: true,
                    edited: false,
                },
            ],
            samples: vec![SceneSample {
                twa: 135.0,
                tws: 14.25,
                bsp: 9.5,
                source: 1,
                id: (1 << 32) | 5,
                hs: 1.5,
                wave_period: 8.5,
                wave_angle: 30.0,
                wave_wind_angle: 15.0,
                current: f32::NAN,
                time: 600.0,
                excluded: true,
                filtered: true,
                band: DayBand::Afternoon,
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
        // 12 header + 2×4 sources + 2×6 nodes + 1×13 samples
        // + (3 + 2 + 2 + 4) + (3 + 1 + 1 + 1) surfaces = 62 words.
        assert_eq!(bytes.len(), 62 * 4);
        assert_eq!(&bytes[0..4], b"PE3D");
        assert_eq!(
            (1..6).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [4, 2, 2, 1, 2]
        );
        assert_eq!(
            i64::from_le_bytes(bytes[24..32].try_into().unwrap()),
            1_753_000_000
        );
        assert_eq!(
            (8..12).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [0x0506_0708, 0x0010_0304, SAMPLES_FULL, 0]
        );
        // Sources: words 12–19.
        assert_eq!(
            (12..20).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [7, 0, 0x4e79a7, 0, 9, 3, 0xe15759, 2]
        );
        // Node positions: words 20–25, then source 26–27, cell 28–29, flags 30–31.
        assert_eq!(
            (20..26).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [52.0, 6.0, 5.9, 90.0, 12.0, 8.4]
        );
        assert_eq!(
            (26..32).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [0, 0, 1, 2 | (1 << 16), FLAG_EDITED, FLAG_EXCLUDED]
        );
        // Sample: position 32–34, source 35, id 36–37, hs 38, current 39,
        // time 40, wave period/angle/wind angle 41–43, flags 44 (the band,
        // afternoon = 2, in bits 8–9).
        assert_eq!(
            (32..35).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [135.0, 14.25, 9.5]
        );
        assert_eq!(
            (35..38).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [1, 5, 1]
        );
        assert_eq!(float(&bytes, 38), 1.5);
        assert!(float(&bytes, 39).is_nan());
        assert_eq!(float(&bytes, 40), 600.0);
        assert_eq!(word(&bytes, 44), FLAG_EXCLUDED | FLAG_FILTERED | (2 << 8));
        assert_eq!(float(&bytes, 41), 8.5);
        assert_eq!(float(&bytes, 42), 30.0);
        assert_eq!(float(&bytes, 43), 15.0);
        // First surface: header 45–47, axes 48–51, values 52–55.
        assert_eq!(
            (45..48).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [0, 2, 2]
        );
        assert_eq!(
            (48..55).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [52.0, 90.0, 6.0, 12.0, 5.9, 7.3, 6.8]
        );
        assert!(float(&bytes, 55).is_nan());
        // The blend: 56–58, then its one-cell grid.
        assert_eq!(
            (56..59).map(|i| word(&bytes, i)).collect::<Vec<_>>(),
            [u32::MAX, 1, 1]
        );
        assert_eq!(
            (59..62).map(|i| float(&bytes, i)).collect::<Vec<_>>(),
            [45.0, 10.0, 6.0]
        );
    }

    /// A flags-only scene carries the samples' flags and nothing else of
    /// them: one word per sample.
    #[test]
    fn a_flags_only_scene_packs_one_word_per_sample() {
        let scene = Scene {
            flags_only: true,
            ..fixture_scene()
        };
        let bytes = pack(&scene);
        assert_eq!(bytes.len(), (59 - 9) * 4);
        assert_eq!(word(&bytes, 10), SAMPLES_FLAGS_ONLY);
        assert_eq!(word(&bytes, 32), FLAG_EXCLUDED | FLAG_FILTERED | (2 << 8));
        assert_eq!(word(&bytes, 33), 0, "the first surface's source");
    }

    /// The same bytes the frontend's unpacking test reads, so the two sides
    /// cannot drift apart. `PE_BLESS=1` rewrites the files after a
    /// deliberate layout change (and the version must change with it).
    #[test]
    fn the_frontend_fixtures_hold_these_bytes() {
        let dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../ui/src/polar/fixtures");
        let full = pack(&fixture_scene());
        let flags = pack(&Scene {
            flags_only: true,
            ..fixture_scene()
        });
        for (name, bytes) in [("scene-v4.bin", full), ("scene-v4-flags.bin", flags)] {
            let path = dir.join(name);
            if std::env::var_os("PE_BLESS").is_some() {
                std::fs::create_dir_all(&dir).unwrap();
                std::fs::write(&path, &bytes).unwrap();
            }
            assert_eq!(std::fs::read(&path).unwrap(), bytes, "{name}");
        }
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
        assert_eq!(bytes.len(), HEADER_BYTES + 200_000 * 13 * 4);
        println!("packed 200k samples in {elapsed:?}");
    }
}
