//! The 2D polar plot over IPC (spec.md 9.2).
//!
//! Rust does every computation; the frontend only draws what it is given.
//! For a chosen true wind speed (or every one a source has, in "all" mode,
//! `tws: None`), each visible polar source gives one curve per pe-polar's
//! bilinear interpolation, swept across that source's own TWA axis — nothing
//! is invented beyond what the source's grid covers (invariant: no
//! extrapolation). A track is not a polar source, so it contributes no
//! curve; its samples are `dots`, every sample of a visible track whose
//! TWS is within the band (a display setting, ±1 kn by default) of the
//! slice. A sample has a place only once it has wind (M9): until then it is
//! left out rather than drawn somewhere invented. The blend is a hook
//! returning `None` until the blend itself lands (M14) — nothing here
//! fabricates one.

use pe_core::source::Source;
use pe_polar::Polar;
use serde::Serialize;
use ts_rs::TS;

use crate::commands::AppState;
use crate::error::Result;

/// How close a sample's TWS may be to the slice and still count as at it,
/// knots, by default (spec.md 9.2). The person changes it in Settings
/// (`Settings::plot_tws_band_kn`).
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

/// One track sample near the slice (spec.md 9.2).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, TS)]
#[ts(export_to = "PolarSampleDot.ts")]
pub struct PolarSampleDot {
    /// The track source it came from.
    pub source_id: u64,
    /// The sample, for selection shared with the map and the 3D view.
    pub sample_id: u64,
    /// True wind angle, degrees, folded to \[0, 180\].
    pub twa: f64,
    /// True wind speed at the sample, knots.
    pub tws: f64,
    /// Boat speed, knots.
    pub bsp: f64,
    /// Taken out by the track's filters (only sent when asked for).
    pub filtered: bool,
    /// Excluded from the blend by hand.
    pub excluded: bool,
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
    /// Samples within the band of the slice (every sample with wind, in
    /// "all"); filtered ones only when asked for.
    pub dots: Vec<PolarSampleDot>,
    /// The band used, knots.
    pub band_kn: f64,
    /// The blend at the slice; `None` until the blend arrives (M14).
    pub blend: Option<PolarCurve>,
}

/// The grid a source's own curve is read from, if it has one. A track is not
/// a polar source (spec.md 9.2: "every visible polar source"); its samples
/// are dots, not curves.
fn source_grid(source: &Source) -> Option<Polar> {
    pe_polar::source_polar(source)
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

/// Every sample of a visible track that has a place in the polar, within
/// `band` of `tws` (every one, for "all").
fn dots_of(
    project: &pe_core::Project,
    tws: Option<f64>,
    band: f64,
    show_filtered: bool,
) -> Vec<PolarSampleDot> {
    let use_corrected = project.blend.use_corrected;
    let mut dots = Vec::new();
    for source in project.sources.iter().filter(|s| s.visible) {
        let Some(track) = source.track() else {
            continue;
        };
        let out = pe_tracks::filtered_out(track, &source.overlay.filters, use_corrected);
        let excluded = &source.overlay.excluded_samples;
        for (sample, filtered) in track.samples.iter().zip(out) {
            if filtered && !show_filtered {
                continue;
            }
            let Some((twa, sample_tws, bsp)) = pe_tracks::polar_point(sample, use_corrected) else {
                continue;
            };
            if tws.is_some_and(|slice| (sample_tws - slice).abs() > band) {
                continue;
            }
            dots.push(PolarSampleDot {
                source_id: source.id.raw(),
                sample_id: sample.id.raw(),
                twa,
                tws: sample_tws,
                bsp,
                filtered,
                excluded: excluded.binary_search(&sample.id).is_ok(),
            });
        }
    }
    dots
}

/// The plot's answer for the open project (spec.md 9.2).
///
/// `tws` chooses the slice; `None` is "all", which draws one curve per
/// visible polar source for every wind speed that source's own grid has,
/// rather than one slice shared by every source. `show_filtered` adds the
/// samples the filters take out, flagged, for drawing dimmed.
#[tauri::command]
pub fn polar_plot(
    state: tauri::State<'_, AppState>,
    tws: Option<f64>,
    show_filtered: Option<bool>,
) -> Result<PolarPlotResult> {
    plot_with(&state, tws, show_filtered.unwrap_or(false))
}

/// [`polar_plot`] without a Tauri handle, filtered samples left out.
pub fn plot(state: &AppState, tws: Option<f64>) -> Result<PolarPlotResult> {
    plot_with(state, tws, false)
}

/// [`polar_plot`] without a Tauri handle.
pub fn plot_with(
    state: &AppState,
    tws: Option<f64>,
    show_filtered: bool,
) -> Result<PolarPlotResult> {
    state.with_session(|session| {
        let band = session.settings.plot_tws_band_kn;
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
            dots: dots_of(&open.project, tws, band, show_filtered),
            band_kn: band,
            // The blend is derived by pe-polar once it exists (M14,
            // invariant 2: never fabricated here in the meantime).
            blend: None,
        })
    })
}

#[cfg(test)]
mod tests {
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::source::SourceKind;
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

    /// A track without wind has no dots; once environment values are on
    /// its samples (put there by hand here, as M9's fetch will), the ones
    /// within the band of the slice appear, with their flags.
    #[test]
    fn samples_become_dots_once_they_have_wind() {
        let app = two_sources();
        let (track_id, ids) = add_track(&app);
        assert!(plot(&app, Some(10.0)).unwrap().dots.is_empty());

        app.with_session(|session| {
            let open = session.require_open()?;
            let source = open.project.source_mut(SourceId(track_id)).unwrap();
            let track = source.track_mut().unwrap();
            for (k, sample) in track.samples.iter_mut().enumerate() {
                sample.tws = Some(9.5 + k as f64);
                sample.twa = Some(60.0 + k as f64);
                sample.speed = Some(6.0);
            }
            Ok(())
        })
        .unwrap();
        let result = plot(&app, Some(10.0)).unwrap();
        // TWS 9.5 and 10.5 are within ±1 kn of 10; 11.5 is not.
        assert_eq!(result.band_kn, DEFAULT_TWS_BAND_KN);
        assert_eq!(
            result.dots.iter().map(|d| d.sample_id).collect::<Vec<_>>(),
            [ids[0], ids[1]]
        );
        assert_eq!(result.dots[0].twa, 60.0);
        assert_eq!(result.dots[0].bsp, 6.0);
        assert!(!result.dots[0].filtered && !result.dots[0].excluded);
        assert_eq!(plot(&app, None).unwrap().dots.len(), 3);

        // A wider band takes the third; a hidden track gives none.
        crate::settings::plot_band_set(&app, 2.0).unwrap();
        assert_eq!(plot(&app, Some(10.0)).unwrap().dots.len(), 3);
        edit_visible(&app, track_id, false);
        assert!(plot(&app, None).unwrap().dots.is_empty());
    }

    /// A track of three fixes 10 minutes apart heading east at 3.6 kn,
    /// imported through the real command; returns its source id and sample
    /// ids.
    fn add_track(app: &AppState) -> (u64, Vec<u64>) {
        // The fixture's sources took ids 1 and 2 by hand.
        app.with_session(|session| {
            session.require_open()?.project.next_id = 10;
            Ok(())
        })
        .unwrap();
        let dir = std::env::temp_dir().join(format!("pe-plot-track-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("boat.csv");
        std::fs::write(
            &path,
            "time,lat,lon\n1753531200,0,0\n1753531800,0,0.01\n1753532400,0,0.02\n",
        )
        .unwrap();
        let result = crate::tracks::import(
            app,
            &[crate::tracks::TrackFileRequest {
                path: path.to_string_lossy().into_owned(),
                mapping: None,
                boats: None,
            }],
        )
        .unwrap();
        assert!(result.failures.is_empty(), "{:?}", result.failures);
        app.with_session(|session| {
            let open = session.require_open()?;
            let source = open.project.sources.last().unwrap();
            let ids = source
                .track()
                .unwrap()
                .samples
                .iter()
                .map(|s| s.id.raw())
                .collect();
            Ok((source.id.raw(), ids))
        })
        .unwrap()
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
