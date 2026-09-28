//! The 2D polar plot over IPC (spec.md 9.2).
//!
//! Rust does every computation; the frontend only draws what it is given.
//! For a chosen true wind speed (or every one a source has, in "all" mode,
//! `tws: None`), each visible polar source gives one curve per pe-polar's
//! bilinear interpolation, swept across that source's own TWA axis — nothing
//! is invented beyond what the source's grid covers (invariant: no
//! extrapolation). A track is not a polar source, so it contributes no
//! curve; its samples will fill `dots` once tracks exist (M8/M9). The blend
//! is a hook returning `None` until the blend itself lands (M14) — nothing
//! here fabricates one.

use pe_core::source::{Source, SourceKind};
use pe_polar::Polar;
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::Result;

/// How close a sample's TWS may be to the slice and still count as at it,
/// knots (spec.md 9.2). Configurable in a later milestone, once samples
/// exist to filter.
pub const DEFAULT_TWS_BAND_KN: f64 = 1.0;

/// One point of a curve.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarCurvePoint.ts")]
pub struct PolarCurvePoint {
    /// True wind angle, degrees, folded to \[0, 180\].
    pub twa: f64,
    /// Boat speed, knots.
    pub bsp: f64,
}

/// One drawn curve: a source's polar at one true wind speed.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarCurve.ts")]
pub struct PolarCurve {
    /// The source it came from; null for the blend.
    pub source_id: Option<u64>,
    /// Its label, for the legend and hover.
    pub label: String,
    /// Its colour, `#rrggbb`.
    pub colour: String,
    /// The true wind speed this curve is at, knots.
    pub tws: f64,
    /// Points with a value, in increasing TWA. Gaps in the source's coverage
    /// are gaps here too: nothing is interpolated across a missing corner.
    pub points: Vec<PolarCurvePoint>,
}

/// One track sample near the slice (spec.md 9.2). Always empty until tracks
/// exist (M8/M9); the shape is ready so drawing it later is only a wire-up.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarSampleDot.ts")]
pub struct PolarSampleDot {
    /// The track source it came from.
    pub source_id: u64,
    /// True wind angle, degrees, folded to \[0, 180\].
    pub twa: f64,
    /// True wind speed at the sample, knots.
    pub tws: f64,
    /// Boat speed, knots.
    pub bsp: f64,
}

/// What the 2D polar plot draws (spec.md 9.2).
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarPlotResult.ts")]
pub struct PolarPlotResult {
    /// The lowest true wind speed any visible polar source covers; null when
    /// none does. The TWS slider's range.
    pub tws_min: Option<f64>,
    /// The highest true wind speed any visible polar source covers.
    pub tws_max: Option<f64>,
    /// One curve per visible polar source at the chosen slice; several per
    /// source, one per wind speed it has, when `tws` was null ("all").
    pub curves: Vec<PolarCurve>,
    /// Samples within [`DEFAULT_TWS_BAND_KN`] of the slice. Empty until
    /// tracks exist.
    pub dots: Vec<PolarSampleDot>,
    /// The blend at the slice; `None` until the blend arrives (M14).
    pub blend: Option<PolarCurve>,
}

/// The grid a source's own curve is read from, if it has one. A track is not
/// a polar source (spec.md 9.2: "every visible polar source"); its samples
/// are dots, not curves.
fn source_grid(source: &Source) -> Option<Polar> {
    match &source.kind {
        SourceKind::Orc { record } => Some(pe_polar::vpp_to_polar(&record.vpp)),
        SourceKind::PolarFile { polar, .. } => Some(polar.clone()),
        SourceKind::Track { .. } => None,
    }
}

/// One curve of `grid` at `tws`, read at every one of the grid's own TWA
/// angles (no extrapolation, gaps stay gaps).
fn curve_at(source: &Source, grid: &Polar, tws: f64) -> PolarCurve {
    let points = grid
        .twa
        .iter()
        .filter_map(|&twa| {
            pe_polar::interpolate(grid, twa, tws).map(|bsp| PolarCurvePoint { twa, bsp })
        })
        .collect();
    PolarCurve {
        source_id: Some(source.id.raw()),
        label: source.label.clone(),
        colour: source.colour.to_string(),
        tws,
        points,
    }
}

/// The plot's answer for the open project (spec.md 9.2).
///
/// `tws` chooses the slice; `None` is "all", which draws one curve per
/// visible polar source for every wind speed that source's own grid has,
/// rather than one slice shared by every source.
#[tauri::command]
pub fn polar_plot(state: tauri::State<'_, AppState>, tws: Option<f64>) -> Result<PolarPlotResult> {
    plot(&state, tws)
}

/// [`polar_plot`] without a Tauri handle.
pub fn plot(state: &AppState, tws: Option<f64>) -> Result<PolarPlotResult> {
    state.with_session(|session| {
        let open = session.require_open()?;
        let grids: Vec<(&Source, Polar)> = open
            .project
            .sources
            .iter()
            .filter(|source| source.visible)
            .filter_map(|source| source_grid(source).map(|grid| (source, grid)))
            .collect();

        let tws_min = grids
            .iter()
            .filter_map(|(_, grid)| grid.tws.first().copied())
            .fold(None, |acc: Option<f64>, value| {
                Some(acc.map_or(value, |current| current.min(value)))
            });
        let tws_max = grids
            .iter()
            .filter_map(|(_, grid)| grid.tws.last().copied())
            .fold(None, |acc: Option<f64>, value| {
                Some(acc.map_or(value, |current| current.max(value)))
            });

        let curves = match tws {
            Some(value) => grids
                .iter()
                .map(|(source, grid)| curve_at(source, grid, value))
                .collect(),
            None => grids
                .iter()
                .flat_map(|(source, grid)| {
                    grid.tws
                        .iter()
                        .map(move |&value| curve_at(source, grid, value))
                })
                .collect(),
        };

        Ok(PolarPlotResult {
            tws_min,
            tws_max,
            curves,
            // No sample exists to place yet (M8/M9 add tracks with fixes);
            // the shape is final so drawing it later needs no IPC change.
            dots: Vec::new(),
            // The blend is derived by pe-polar once it exists (M14,
            // invariant 2: never fabricated here in the meantime).
            blend: None,
        })
    })
}

#[cfg(test)]
mod tests {
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::{Colour, Command, SourceId};

    use super::*;
    use crate::error::AppError;
    use crate::projects;

    fn polar_file_source(
        id: u64,
        label: &str,
        twa: Vec<f64>,
        tws: Vec<f64>,
        bsp: Vec<Vec<Option<f64>>>,
    ) -> Source {
        Source::new(
            SourceId(id),
            label,
            Colour::parse("#4e79a7").unwrap(),
            SourceKind::PolarFile {
                format: PolarFileFormat::Expedition,
                file_name: format!("{label}.txt"),
                polar: PolarGrid { twa, tws, bsp },
            },
        )
    }

    /// A throwaway state rooted under a name unique to this test run.
    fn state(label: &str) -> AppState {
        let dir = std::env::temp_dir().join(format!(
            "pe-polar-plot-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_default()
        ));
        AppState::new(crate::paths::AppPaths::in_directory(&dir).expect("paths"))
    }

    /// A project with two polar-file sources of different axes, ids 1 and 2.
    fn two_sources() -> AppState {
        let app = state("two-sources");
        projects::create(&app, "Plot".to_owned(), None, false).expect("create");
        app.with_session(|session| {
            let open = session.require_open()?;
            let a = polar_file_source(
                1,
                "A",
                vec![40.0, 90.0, 150.0],
                vec![6.0, 12.0],
                vec![
                    vec![Some(4.0), Some(6.0)],
                    vec![Some(6.0), Some(8.0)],
                    vec![Some(5.0), Some(9.0)],
                ],
            );
            let b = polar_file_source(
                2,
                "B",
                vec![45.0, 135.0],
                vec![10.0],
                vec![vec![Some(5.0)], vec![Some(7.0)]],
            );
            open.apply(Command::AddSource {
                index: 0,
                source: Box::new(a),
            })?;
            open.apply(Command::AddSource {
                index: 1,
                source: Box::new(b),
            })?;
            Ok(())
        })
        .expect("add sources");
        app
    }

    #[test]
    fn a_slice_gives_one_curve_per_visible_source_at_that_tws() {
        let app = two_sources();
        let result = plot(&app, Some(9.0)).unwrap();
        assert_eq!(result.curves.len(), 2);
        let a = result.curves.iter().find(|c| c.label == "A").unwrap();
        assert_eq!(a.tws, 9.0);
        // 40°: 5.0, 90°: 7.0, 150°: 7.0 at 9 kn (halfway between the 6 and 12
        // kn columns).
        assert_eq!(
            a.points,
            vec![
                PolarCurvePoint {
                    twa: 40.0,
                    bsp: 5.0
                },
                PolarCurvePoint {
                    twa: 90.0,
                    bsp: 7.0
                },
                PolarCurvePoint {
                    twa: 150.0,
                    bsp: 7.0
                },
            ]
        );
        let b = result.curves.iter().find(|c| c.label == "B").unwrap();
        // B has no 9 kn coverage (its only column is 10 kn): nothing is
        // extrapolated, so its curve is empty at this slice.
        assert!(b.points.is_empty());
    }

    #[test]
    fn all_mode_gives_one_curve_per_source_per_native_wind_speed() {
        let app = two_sources();
        let result = plot(&app, None).unwrap();
        // A has two TWS columns, B has one: three curves in all.
        assert_eq!(result.curves.len(), 3);
        let a_speeds: Vec<f64> = result
            .curves
            .iter()
            .filter(|c| c.label == "A")
            .map(|c| c.tws)
            .collect();
        assert_eq!(a_speeds, vec![6.0, 12.0]);
    }

    #[test]
    fn the_domain_spans_every_visible_sources_tws_axis() {
        let app = two_sources();
        let result = plot(&app, Some(9.0)).unwrap();
        assert_eq!(result.tws_min, Some(6.0));
        assert_eq!(result.tws_max, Some(12.0));
    }

    #[test]
    fn a_hidden_source_is_left_out_of_every_plot() {
        let app = two_sources();
        edit_visible(&app, 1, false);
        let result = plot(&app, None).unwrap();
        assert!(result.curves.iter().all(|c| c.label != "A"));
        assert_eq!(result.tws_min, Some(10.0));
    }

    fn edit_visible(app: &AppState, id: u64, visible: bool) {
        crate::edit::source_visible_set(app, id, visible).unwrap();
    }

    #[test]
    fn a_track_source_gives_no_curve() {
        let app = two_sources();
        app.with_session(|session| {
            let open = session.require_open()?;
            let origin = pe_core::track::TrackOrigin::File {
                name: "race.gpx".to_owned(),
                boat_name: None,
            };
            let track = Source::new(
                SourceId(3),
                "Track",
                Colour::parse("#e15759").unwrap(),
                SourceKind::Track {
                    track: Box::new(pe_core::track::Track::new(pe_core::TrackId(3), origin)),
                },
            );
            open.apply(Command::AddSource {
                index: 2,
                source: Box::new(track),
            })
        })
        .unwrap();
        let result = plot(&app, None).unwrap();
        assert!(result.curves.iter().all(|c| c.label != "Track"));
        assert!(result.dots.is_empty());
    }

    #[test]
    fn the_blend_is_none_until_it_arrives() {
        let app = two_sources();
        assert_eq!(plot(&app, None).unwrap().blend, None);
    }

    #[test]
    fn no_open_project_is_refused() {
        let app = state("none");
        assert!(matches!(plot(&app, None), Err(AppError::NoProjectOpen)));
    }

    #[test]
    fn nothing_visible_gives_an_empty_domain_and_no_curves() {
        let app = two_sources();
        edit_visible(&app, 1, false);
        edit_visible(&app, 2, false);
        let result = plot(&app, Some(10.0)).unwrap();
        assert!(result.curves.is_empty());
        assert_eq!(result.tws_min, None);
        assert_eq!(result.tws_max, None);
    }
}
