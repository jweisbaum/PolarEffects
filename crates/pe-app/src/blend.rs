//! The blend, its settings and its export over IPC (spec.md 8, 12, 6).
//!
//! The rule itself is `pe_polar::blend` (one function, spec.md 12.3); this
//! module reads every visible source onto the output grid for it — a polar
//! source resampled with its edits in and every cell read from an excluded
//! node left out, a track through its segment with its per-cell counts —
//! and hands the answer to the views, the source list and export.
//!
//! **Export always recomputes** (invariant 2): [`fresh`] derives every
//! source from scratch, never from the session's cache, and the bytes it
//! writes are the writer's, which depend on nothing but the project
//! (invariant 5). A custom export grid is the blend resampled onto it
//! (bilinear, never extrapolated, spec.md 12.2): tracks are binned on the
//! project's grid, so that is the grid the blend is made on.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use pe_core::command::BLEND_SETTINGS_LABEL;
use pe_core::project::{BlendSettings, MAX_GRID_TWS_KN, OutputGrid, validate_grid_axis};
use pe_core::{Colour, Command, Project};
use pe_polar::blend::on_grid;
use pe_polar::{
    Blend, BlendOptions, BlendSource, CellOrigin, Confidence, ExportProblem, Polar, PolarFileFormat,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::commands::AppState;
use crate::derived::{Derivations, Derived};
use crate::error::{AppError, Context, Result};
use crate::polar_edit::{parse_statistic, statistic_name};
use crate::projects::ProjectSummary;

/// The blend of `project` from each visible source's derived data.
pub fn assemble(project: &Project, derived: &BTreeMap<u64, Arc<Derived>>) -> Blend {
    let grid = &project.grid;
    let read: Vec<(u64, f64, Polar, Option<&Derived>)> = project
        .sources
        .iter()
        .filter(|source| source.visible)
        .filter_map(|source| {
            let data = derived.get(&source.id.raw())?;
            let (polar, track) = if data.track.is_some() {
                // A segment is already on the output grid, its overrides in
                // and its excluded nodes empty.
                (data.blend.clone(), Some(&**data))
            } else {
                (
                    on_grid(&data.edited, &source.overlay, &grid.twa, &grid.tws),
                    None,
                )
            };
            Some((source.id.raw(), source.weight, polar, track))
        })
        .collect();
    let sources: Vec<BlendSource<'_>> = read
        .iter()
        .map(|(id, weight, polar, track)| BlendSource {
            id: *id,
            grid: polar,
            weight: *weight,
            confidence: match track.and_then(|d| d.track.as_ref()) {
                Some(placed) => Confidence::Samples(&placed.segment.count),
                None => Confidence::Full,
            },
        })
        .collect();
    pe_polar::blend(
        &grid.twa,
        &grid.tws,
        &sources,
        &BlendOptions {
            n_full: project.blend.n_full,
            smoothing: project.blend.smoothing,
        },
    )
}

/// The blend derived from scratch: what export writes (invariant 2).
pub fn fresh(project: &Project) -> Blend {
    let derived = Derivations::default().visible(project);
    assemble(project, &derived)
}

/// The Blend entry and the Blend settings dialog (spec.md 8, 12).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "BlendSummary.ts")]
pub struct BlendSummary {
    /// Its colour in every plot, `#rrggbb`.
    pub colour: String,
    /// Whether it is drawn.
    pub visible: bool,
    /// Cells with direct evidence (spec.md 12.3).
    pub direct: u32,
    /// Cells filled by interpolation, and the 0° row.
    pub filled: u32,
    /// Cells nothing reaches.
    pub empty: u32,
    /// The output grid's TWA axis, degrees (spec.md 12.2).
    pub twa: Vec<f64>,
    /// The output grid's TWS axis, knots.
    pub tws: Vec<f64>,
    /// Samples a track cell needs (spec.md 12.1).
    pub min_samples: u32,
    /// Samples at which a track cell counts fully (spec.md 12.3).
    pub n_full: u32,
    /// Smoothing over the filled grid.
    pub smoothing: bool,
    /// The statistic new tracks start with: `median`, `mean`, `p75`, `p90`.
    pub default_statistic: String,
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

impl BlendSummary {
    /// The summary of `project` with its blend.
    pub fn of(project: &Project, blend: &Blend) -> Self {
        let coverage = blend.coverage();
        let settings = &project.blend;
        Self {
            colour: settings.colour.to_string(),
            visible: settings.visible,
            direct: count(coverage.direct),
            filled: count(coverage.filled),
            empty: count(coverage.empty),
            twa: project.grid.twa.clone(),
            tws: project.grid.tws.clone(),
            min_samples: settings.min_samples,
            n_full: settings.n_full,
            smoothing: settings.smoothing,
            default_statistic: statistic_name(settings.default_statistic).to_owned(),
        }
    }
}

/// Changes the blend settings with `change`, as one undoable entry.
fn settings_set(
    state: &AppState,
    change: impl FnOnce(&mut BlendSettings) -> Result<()>,
) -> Result<ProjectSummary> {
    crate::edit::apply(state, |project| {
        let before = project.blend.clone();
        let mut after = before.clone();
        change(&mut after)?;
        Ok((after != before).then(|| Command::SetBlendSettings {
            before: Box::new(before),
            after: Box::new(after),
        }))
    })
}

/// Shows or hides the blend in every plot (spec.md 8). Export is unchanged.
#[tauri::command]
pub fn set_blend_visible(
    state: tauri::State<'_, AppState>,
    visible: bool,
) -> Result<ProjectSummary> {
    blend_visible_set(&state, visible)
}

/// [`set_blend_visible`] without a Tauri handle.
pub fn blend_visible_set(state: &AppState, visible: bool) -> Result<ProjectSummary> {
    settings_set(state, |settings| {
        settings.visible = visible;
        Ok(())
    })
}

/// Changes the blend's colour (spec.md 8). `colour` is `#rrggbb`.
#[tauri::command]
pub fn set_blend_colour(
    state: tauri::State<'_, AppState>,
    colour: String,
) -> Result<ProjectSummary> {
    blend_colour_set(&state, &colour)
}

/// [`set_blend_colour`] without a Tauri handle.
pub fn blend_colour_set(state: &AppState, colour: &str) -> Result<ProjectSummary> {
    let colour = Colour::parse(colour)?;
    settings_set(state, |settings| {
        settings.colour = colour;
        Ok(())
    })
}

/// What the Blend settings dialog applies (spec.md 12).
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export_to = "BlendSettingsInput.ts")]
pub struct BlendSettingsInput {
    /// Output grid TWA axis, degrees.
    pub twa: Vec<f64>,
    /// Output grid TWS axis, knots.
    pub tws: Vec<f64>,
    /// Samples a track cell needs.
    pub min_samples: u32,
    /// Samples at which a track cell counts fully.
    pub n_full: u32,
    /// Smooth the filled grid.
    pub smoothing: bool,
    /// `median`, `mean`, `p75` or `p90`.
    pub default_statistic: String,
    /// Feed the polar from current-corrected values.
    pub use_corrected: bool,
    /// Include Stokes drift in the global merged current.
    pub stokes_drift: bool,
}

/// Applies the Blend settings dialog as one undoable entry: the output grid
/// and the settings together, or whichever of them changed.
#[tauri::command]
pub fn set_blend_settings(
    state: tauri::State<'_, AppState>,
    settings: BlendSettingsInput,
) -> Result<ProjectSummary> {
    blend_settings_set(&state, settings)
}

/// [`set_blend_settings`] without a Tauri handle.
pub fn blend_settings_set(state: &AppState, input: BlendSettingsInput) -> Result<ProjectSummary> {
    let statistic = parse_statistic(&input.default_statistic).ok_or(AppError::BadOption {
        field: "Default statistic",
        value: input.default_statistic.clone(),
    })?;
    let grid = OutputGrid {
        twa: input.twa.clone(),
        tws: input.tws.clone(),
    };
    grid.validate()?;
    crate::edit::apply(state, |project| {
        let before = project.blend.clone();
        let after = BlendSettings {
            min_samples: input.min_samples,
            n_full: input.n_full,
            smoothing: input.smoothing,
            use_corrected: input.use_corrected,
            include_stokes_drift: input.stokes_drift,
            default_statistic: statistic,
            ..before.clone()
        };
        after.validate()?;
        let mut commands = Vec::new();
        if project.grid != grid {
            commands.push(Command::SetOutputGrid {
                before: project.grid.clone(),
                after: grid,
            });
        }
        if after != before {
            commands.push(Command::SetBlendSettings {
                before: Box::new(before),
                after: Box::new(after),
            });
        }
        Ok(match commands.len() {
            0 => None,
            1 => commands.pop(),
            _ => Some(Command::Batch {
                label: BLEND_SETTINGS_LABEL.to_owned(),
                commands,
            }),
        })
    })
}

/// The axes an export is written on, when not the project's output grid.
#[derive(Debug, Clone, PartialEq, Deserialize, TS)]
#[ts(export_to = "ExportAxes.ts")]
pub struct ExportAxes {
    /// TWA values, degrees.
    pub twa: Vec<f64>,
    /// TWS values, knots.
    pub tws: Vec<f64>,
}

/// Why an export would be refused (see `pe_polar::export`).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ExportProblemView.ts")]
pub struct ExportProblemView {
    /// `empty`, `axis-collision`, `out-of-range` or `too-fast`.
    pub code: String,
    /// `twa` or `tws`, for an axis problem.
    pub axis: Option<String>,
    /// The first value of a collision, or the value out of range.
    pub first: Option<f64>,
    /// The second value of a collision.
    pub second: Option<f64>,
    /// What both values of a collision would be written as.
    pub written: Option<String>,
    /// Where a boat speed is too fast: TWA, TWS and the speed.
    pub cell: Option<(f64, f64, f64)>,
}

impl From<&ExportProblem> for ExportProblemView {
    fn from(problem: &ExportProblem) -> Self {
        let mut view = Self {
            code: problem.code().to_owned(),
            axis: None,
            first: None,
            second: None,
            written: None,
            cell: None,
        };
        match problem {
            ExportProblem::Empty => {}
            ExportProblem::AxisCollision {
                axis,
                first,
                second,
                written,
            } => {
                view.axis = Some(axis.code().to_owned());
                view.first = Some(*first);
                view.second = Some(*second);
                view.written = Some(written.clone());
            }
            ExportProblem::OutOfRange { axis, value } => {
                view.axis = Some(axis.code().to_owned());
                view.first = Some(*value);
            }
            ExportProblem::TooFast { twa, tws, bsp } => view.cell = Some((*twa, *tws, *bsp)),
        }
        view
    }
}

/// What the export dialog shows before saving (spec.md 12): the grid that
/// would be written and the file's text, or why it cannot be.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ExportPreview.ts")]
pub struct ExportPreview {
    /// `expedition`, `adrena` or `csv`.
    pub format: String,
    /// TWA axis, degrees.
    pub twa: Vec<f64>,
    /// TWS axis, knots.
    pub tws: Vec<f64>,
    /// Boat speed per cell, `[twa][tws]`.
    pub bsp: Vec<Vec<Option<f64>>>,
    /// Per cell on the project's grid: `direct`, `filled` or `empty`; null
    /// on a custom grid, which is the blend resampled.
    pub origin: Option<Vec<Vec<String>>>,
    /// The file as it would be written; empty when refused.
    pub text: String,
    /// Why it would be refused.
    pub problem: Option<ExportProblemView>,
}

fn parse_format(format: &str) -> Result<PolarFileFormat> {
    match format {
        "expedition" => Ok(PolarFileFormat::Expedition),
        "adrena" => Ok(PolarFileFormat::Adrena),
        "csv" => Ok(PolarFileFormat::Csv),
        other => Err(AppError::BadOption {
            field: "Export format",
            value: other.to_owned(),
        }),
    }
}

fn format_name(format: PolarFileFormat) -> &'static str {
    match format {
        PolarFileFormat::Expedition => "expedition",
        PolarFileFormat::Adrena => "adrena",
        PolarFileFormat::Csv => "csv",
    }
}

fn origin_name(origin: CellOrigin) -> &'static str {
    match origin {
        CellOrigin::Direct => "direct",
        CellOrigin::Filled => "filled",
        CellOrigin::Empty => "empty",
    }
}

/// The grid an export writes: the blend on the project grid, or resampled
/// onto custom axes (which must pass the output grid's own rules).
pub fn export_grid(project: &Project, axes: Option<&ExportAxes>) -> Result<(Blend, Option<Polar>)> {
    let blend = fresh(project);
    let Some(axes) = axes else {
        return Ok((blend, None));
    };
    validate_grid_axis(&axes.twa, "TWA", 180.0)?;
    validate_grid_axis(&axes.tws, "TWS", MAX_GRID_TWS_KN)?;
    let polar = pe_polar::resample(&blend.polar, &axes.twa, &axes.tws);
    Ok((blend, Some(polar)))
}

/// The preview of an export of `project` (pure; no session).
pub fn preview_of(
    project: &Project,
    format: &str,
    axes: Option<&ExportAxes>,
) -> Result<ExportPreview> {
    let format = parse_format(format)?;
    let (blend, custom) = export_grid(project, axes)?;
    let origin = custom.is_none().then(|| {
        blend
            .origin
            .iter()
            .map(|row| row.iter().map(|o| origin_name(*o).to_owned()).collect())
            .collect()
    });
    let polar = custom.unwrap_or(blend.polar);
    let (text, problem) = match pe_polar::export(format, &polar) {
        Ok(text) => (text, None),
        Err(problem) => (String::new(), Some(ExportProblemView::from(&problem))),
    };
    Ok(ExportPreview {
        format: format_name(format).to_owned(),
        twa: polar.twa,
        tws: polar.tws,
        bsp: polar.bsp,
        origin,
        text,
        problem,
    })
}

/// The bytes an export of `project` writes (pure; no session): recomputed
/// from the sources, refused with the problem named.
pub fn export_bytes(project: &Project, format: &str, axes: Option<&ExportAxes>) -> Result<Vec<u8>> {
    let format = parse_format(format)?;
    let (blend, custom) = export_grid(project, axes)?;
    let polar = custom.unwrap_or(blend.polar);
    pe_polar::export(format, &polar)
        .map(String::into_bytes)
        .map_err(|problem| AppError::Export {
            code: problem.code(),
            message: problem.to_string(),
        })
}

/// The export dialog's preview (spec.md 12).
#[tauri::command]
pub fn export_preview(
    state: tauri::State<'_, AppState>,
    format: String,
    axes: Option<ExportAxes>,
) -> Result<ExportPreview> {
    preview(&state, &format, axes.as_ref())
}

/// [`export_preview`] without a Tauri handle.
pub fn preview(state: &AppState, format: &str, axes: Option<&ExportAxes>) -> Result<ExportPreview> {
    state.with_session(|session| preview_of(&session.require_open()?.project, format, axes))
}

/// What an export wrote.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "ExportResult.ts")]
pub struct ExportResult {
    /// The file written.
    pub path: String,
    /// Its size.
    pub bytes: u32,
}

/// Writes the blend to `path` in `format` (spec.md 6, 12), recomputed from
/// the sources (invariant 2), on the project's grid or `axes`.
#[tauri::command]
pub fn export_polar(
    state: tauri::State<'_, AppState>,
    path: String,
    format: String,
    axes: Option<ExportAxes>,
) -> Result<ExportResult> {
    export_to(&state, &path, &format, axes.as_ref())
}

/// [`export_polar`] without a Tauri handle.
pub fn export_to(
    state: &AppState,
    path: &str,
    format: &str,
    axes: Option<&ExportAxes>,
) -> Result<ExportResult> {
    let bytes = state
        .with_session(|session| export_bytes(&session.require_open()?.project, format, axes))?;
    let target = PathBuf::from(path);
    pe_core::io::write_atomic(&target, &bytes).doing("write the polar to", path)?;
    Ok(ExportResult {
        path: path.to_owned(),
        bytes: count(bytes.len()),
    })
}
