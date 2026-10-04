//! Sample filters (spec.md 7.6) and where a sample sits in the polar.
//!
//! A filter never deletes: it marks which samples are out, and the plots
//! draw those dimmed when asked. A sample lacking the value an active
//! filter reads (no wave height under a wave-height range, no speed under a
//! minimum boat speed) is out, since nothing shows it passes; with no
//! filter on a quantity, a missing value does not matter.

use pe_core::source::{DirectionRange, Range, SampleFilters, WaveDirectionFilter, WaveSector};
use pe_core::track::{Sample, Track};

use crate::geo::{angle_between, wrap_360};

/// A sample's place in the polar: (TWA °, TWS kn, BSP kn), water-relative
/// where a current correction exists and `use_corrected` is on (spec.md
/// 7.5, D13), ground-relative otherwise. `None` until the sample has wind
/// (M9): a sample without wind has no place and is not drawn, rather than
/// being drawn somewhere invented.
pub fn polar_point(sample: &Sample, use_corrected: bool) -> Option<(f64, f64, f64)> {
    let corrected = use_corrected
        .then(|| {
            Some((
                sample.twa_corrected?,
                sample.tws_corrected?,
                sample.bsp_corrected?,
            ))
        })
        .flatten();
    corrected.or_else(|| Some((sample.twa?, sample.wind_speed()?, sample.speed?)))
}

/// The boat speed a sample counts with, knots.
pub fn boat_speed(sample: &Sample, use_corrected: bool) -> Option<f64> {
    let corrected = if use_corrected {
        sample.bsp_corrected
    } else {
        None
    };
    corrected.or(sample.speed)
}

fn heading(sample: &Sample, use_corrected: bool) -> Option<f64> {
    let corrected = if use_corrected {
        sample.heading_corrected
    } else {
        None
    };
    corrected.or(sample.heading)
}

fn in_range(range: &Range, value: Option<f64>) -> bool {
    let Some(value) = value else { return false };
    range.min.is_none_or(|min| value >= min) && range.max.is_none_or(|max| value <= max)
}

fn in_direction_range(range: &DirectionRange, direction: Option<f64>) -> bool {
    direction.is_some_and(|d| in_compass_range(d, range.from, range.to))
}

/// Speed made good to windward, knots: positive upwind, negative downwind.
fn vmg(sample: &Sample, use_corrected: bool) -> Option<f64> {
    let (twa, _, bsp) = polar_point(sample, use_corrected)?;
    Some(bsp * twa.to_radians().cos())
}

fn wind_direction(sample: &Sample, use_corrected: bool) -> Option<f64> {
    if use_corrected {
        sample.twd_from_corrected.or(sample.wind_direction())
    } else {
        sample.wind_direction()
    }
}

/// The relative-bearing sector waves come from, off the bow (0° head seas,
/// 180° following): head below 30°, bow 30–60°, beam 60–120°, quarter
/// 120–150°, following from 150°.
pub fn wave_sector(wave_angle: f64) -> WaveSector {
    match wave_angle {
        a if a < 30.0 => WaveSector::Head,
        a if a < 60.0 => WaveSector::Bow,
        a if a < 120.0 => WaveSector::Beam,
        a if a < 150.0 => WaveSector::Quarter,
        _ => WaveSector::Following,
    }
}

/// Whether a compass direction lies clockwise from `from` to `to`.
fn in_compass_range(direction: f64, from: f64, to: f64) -> bool {
    wrap_360(direction - from) <= wrap_360(to - from)
}

fn wave_passes(filter: &WaveDirectionFilter, sample: &Sample) -> bool {
    match filter {
        WaveDirectionFilter::Sectors { sectors } => sample
            .wave_angle
            .is_some_and(|a| sectors.contains(&wave_sector(a))),
        WaveDirectionFilter::Relative { range } => in_range(range, sample.wave_angle),
        WaveDirectionFilter::Cog { range } => in_range(
            range,
            sample
                .heading
                .zip(sample.wave_from)
                .map(|(h, w)| angle_between(h, w)),
        ),
        WaveDirectionFilter::Absolute { range } => sample
            .wave_from
            .is_some_and(|w| in_compass_range(w, range.from, range.to)),
    }
}

/// The main wave sliders apply to the same samples as the blend. Use the
/// scene's f32 boundary precision so dragging an inclusive bound to a displayed
/// observation keeps that observation in both Rust and the preview.
pub fn wave_ranges_exclude(
    sample: &Sample,
    ranges: &pe_core::source::WaveRanges,
    corrected: bool,
) -> bool {
    let active = |r: &Range| r.min.is_some() || r.max.is_some();
    let angle = if active(&ranges.angle_deg) {
        sample
            .wave_from
            .zip(heading(sample, corrected))
            .map(|(w, h)| angle_between(w, h))
    } else {
        None
    };
    [
        (&ranges.height_m, sample.hs_m),
        (&ranges.angle_deg, angle),
        (&ranges.period_s, sample.wave_period_s),
    ]
    .into_iter()
    .any(|(r, value)| {
        active(r)
            && !value.is_some_and(|value| {
                r.min.is_none_or(|min| value as f32 >= min as f32)
                    && r.max.is_none_or(|max| value as f32 <= max as f32)
            })
    })
}

/// Which samples the filters take out, one flag per sample in order
/// (`true`: filtered out). Hand exclusions are separate (spec.md 10.3).
pub fn filtered_out(track: &Track, filters: &SampleFilters, use_corrected: bool) -> Vec<bool> {
    let samples = &track.samples;
    let tacks = tack_flags(track, filters, use_corrected);
    let changes = change_flags(track, filters, use_corrected);
    let no_tide = |index: Option<u16>| {
        index
            .and_then(|i| track.env_meta.datasets.get(usize::from(i)))
            .is_some_and(|d| d.has_tide == Some(false))
    };
    samples
        .iter()
        .enumerate()
        .map(|(k, sample)| {
            let (twa, tws) = match polar_point(sample, use_corrected) {
                Some((twa, tws, _)) => (Some(twa), Some(tws)),
                None => (sample.twa, sample.wind_speed()),
            };
            let bsp = boat_speed(sample, use_corrected);
            let passes = !tacks[k]
                && !changes[k]
                && (!filters.exclude_unknown_wave
                    || (sample.hs_m.is_some() && sample.wave_from.is_some()))
                && (!filters.exclude_unknown_current
                    || (sample.current_speed.is_some() && sample.current_toward.is_some()))
                && filters
                    .heading_deg
                    .as_ref()
                    .is_none_or(|r| in_direction_range(r, heading(sample, use_corrected)))
                && filters
                    .cog_deg
                    .as_ref()
                    .is_none_or(|r| in_direction_range(r, sample.heading))
                && filters
                    .vmg_kn
                    .as_ref()
                    .is_none_or(|r| in_range(r, vmg(sample, use_corrected)))
                && filters
                    .twd_deg
                    .as_ref()
                    .is_none_or(|r| in_direction_range(r, wind_direction(sample, use_corrected)))
                && filters
                    .wave_height_m
                    .as_ref()
                    .is_none_or(|r| in_range(r, sample.hs_m))
                && filters
                    .wave_direction
                    .as_ref()
                    .is_none_or(|f| wave_passes(f, sample))
                && filters
                    .current_speed_kn
                    .as_ref()
                    .is_none_or(|r| in_range(r, sample.current_speed))
                && filters.tws_kn.as_ref().is_none_or(|r| in_range(r, tws))
                && filters.twa_deg.as_ref().is_none_or(|r| in_range(r, twa))
                && filters.time_window.as_ref().is_none_or(|w| {
                    w.start.is_none_or(|s| sample.t >= s) && w.end.is_none_or(|e| sample.t <= e)
                })
                && filters
                    .min_bsp_kn
                    .is_none_or(|min| bsp.is_some_and(|b| b >= min))
                && filters
                    .max_bsp_kn
                    .is_none_or(|max| bsp.is_some_and(|b| b <= max))
                && !(filters.exclude_no_tide && no_tide(sample.current_dataset));
            !passes
        })
        .collect()
}

/// The sample on either side of every tack and gybe, when the filter asks
/// for them: the wind crosses the bow or the stern where the tack (the side
/// the wind is on) differs between a sample and the last one that had a
/// tack. Head to wind or dead downwind in between may bridge a tack;
/// missing wind or motion cannot establish one, nor can a gap longer than
/// the track's maximum.
fn tack_flags(track: &Track, filters: &SampleFilters, corrected: bool) -> Vec<bool> {
    let mut out = vec![false; track.samples.len()];
    if !filters.exclude_tacks {
        return out;
    }
    let side = |sample: &Sample| {
        if corrected && sample.twa_corrected.is_some() {
            sample.tack_corrected
        } else {
            sample.tack
        }
    };
    let mut previous: Option<usize> = None;
    for (k, sample) in track.samples.iter().enumerate() {
        if let Some(p) = previous
            && sample.t.saturating_sub(track.samples[p].t) > track.derivation.max_gap_s
        {
            previous = None;
        }
        if side(sample).is_some() {
            if let Some(p) = previous
                && side(&track.samples[p]) != side(sample)
            {
                out[p] = true;
                out[k] = true;
            }
            previous = Some(k);
        } else if polar_point(sample, corrected).is_none() {
            previous = None;
        }
    }
    out
}

/// Compare each observation only with its immediately previous and next fixes.
/// Read original observations so another filter cannot hide a change, and never
/// bridge missing values or a gap longer than the track's maximum gap. O(n).
fn change_flags(track: &Track, filters: &SampleFilters, corrected: bool) -> Vec<bool> {
    let mut out = vec![false; track.samples.len()];
    type Metric = fn(&Sample, bool) -> Option<f64>;
    let wind_speed: Metric = |s, c| {
        if c {
            s.tws_corrected.or(s.wind_speed())
        } else {
            s.wind_speed()
        }
    };
    let wind_direction: Metric = |s, c| {
        if c {
            s.twd_from_corrected.or(s.wind_direction())
        } else {
            s.wind_direction()
        }
    };
    for (limit, circular, metric) in [
        (filters.max_heading_change_deg, true, heading as Metric),
        (
            filters.max_awa_change_deg,
            true,
            (|s: &Sample, corrected| s.apparent_wind_angle(corrected)) as Metric,
        ),
        (filters.max_wind_speed_change_kn, false, wind_speed),
        (filters.max_wind_direction_change_deg, true, wind_direction),
    ] {
        let Some(limit) = limit else { continue };
        if out.iter().all(|excluded| *excluded) {
            break;
        }
        let values: Vec<_> = track
            .samples
            .iter()
            .map(|s| {
                metric(s, corrected)
                    .filter(|v| v.is_finite() && *v >= 0.0)
                    .map(|v| (if circular { wrap_360(v) } else { v }) + 0.0)
            })
            .collect();
        let distance = |a: f64, b: f64| {
            let d = (a - b).abs();
            if circular {
                // Both angles are already in [0, 360); avoid re-wrapping every
                // comparison between neighbours.
                d.min(360.0 - d)
            } else {
                d
            }
        };
        for k in 1..values.len() {
            if track.samples[k].t.saturating_sub(track.samples[k - 1].t)
                <= track.derivation.max_gap_s
                && let (Some(a), Some(b)) = (values[k - 1], values[k])
                && distance(a, b) > limit
            {
                out[k - 1] = true;
                out[k] = true;
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use pe_core::SampleId;
    use pe_core::TrackId;
    use pe_core::source::{DirectionRange, TimeWindow};
    use pe_core::track::{DatasetRecord, Fix, TrackOrigin, ValueOrigin};

    use super::*;

    fn track(values: &[(i64, Option<f64>, Option<f64>)]) -> Track {
        let mut track = Track::new(
            TrackId(1),
            TrackOrigin::File {
                name: "t.csv".to_owned(),
                boat_name: None,
            },
        );
        for (i, (t, heading, speed)) in values.iter().enumerate() {
            let fix = Fix {
                tws: None,
                twd_from: None,
                t: *t,
                lat: 0.0,
                lon: 0.0,
                cog: None,
                sog: None,
            };
            let mut sample = Sample::at(SampleId(i as u64 + 10), i as u32, &fix);
            sample.heading = *heading;
            sample.heading_origin = heading.map(|_| ValueOrigin::Derived);
            sample.speed = *speed;
            sample.speed_origin = speed.map(|_| ValueOrigin::Given);
            track.fixes.push(fix);
            track.samples.push(sample);
        }
        track
    }

    fn none() -> SampleFilters {
        SampleFilters {
            min_bsp_kn: None,
            max_heading_change_deg: None,
            ..SampleFilters::default()
        }
    }

    #[test]
    fn changes_compare_only_neighbours_wrap_north_and_do_not_bridge_gaps() {
        let mut t = track(&[
            (0, Some(350.0), Some(6.0)),
            (30, Some(355.0), Some(6.0)),
            (60, Some(5.0), Some(6.0)),
            (90, Some(10.0), Some(6.0)),
            (600, Some(180.0), Some(6.0)),
        ]);
        t.derivation.max_gap_s = 120;
        let mut f = SampleFilters {
            max_heading_change_deg: Some(10.0),
            ..none()
        };
        assert_eq!(filtered_out(&t, &f, false), [false; 5]); // equality passes
        f.max_heading_change_deg = Some(9.0);
        assert_eq!(
            filtered_out(&t, &f, false),
            [false, true, true, false, false]
        );
        f.max_heading_change_deg = None;
        f.max_wind_speed_change_kn = Some(2.0);
        for (s, speed) in t.samples.iter_mut().zip([10.0, 11.0, 13.0, 13.0, 30.0]) {
            s.tws = Some(speed);
            s.twd_from = Some(350.0);
            s.relate();
        }
        assert_eq!(filtered_out(&t, &f, false), [false; 5]); // equality passes
        f.max_wind_speed_change_kn = Some(1.5);
        assert_eq!(
            filtered_out(&t, &f, false),
            [false, true, true, false, false]
        );
        f.max_wind_speed_change_kn = None;
        f.max_wind_direction_change_deg = Some(9.0);
        for (s, direction) in t.samples.iter_mut().zip([350.0, 355.0, 5.0, 10.0, 180.0]) {
            s.twd_from = Some(direction);
            s.relate();
        }
        assert_eq!(
            filtered_out(&t, &f, false),
            [false, true, true, false, false]
        );
    }

    #[test]
    fn awa_is_apparent_bearing_and_supplied_wind_can_be_selected() {
        let mut t = track(&[
            (0, Some(0.0), Some(10.0)),
            (60, Some(0.0), Some(10.0)),
            (120, Some(0.0), Some(10.0)),
        ]);
        for s in &mut t.samples {
            s.tws = Some(10.0);
            s.twd_from = Some(90.0);
            s.relate();
        }
        // 10 kn from east plus 10 kn apparent headwind gives 45°.
        assert_eq!(t.samples[0].apparent_wind_angle(false), Some(45.0));
        // Removing an east-going current turns the bow west of north.
        // Apparent air velocity stays the same; its angle follows that bow.
        t.samples[0].current_speed = Some(5.0);
        t.samples[0].current_toward = Some(90.0);
        t.samples[0].relate();
        assert_eq!(t.samples[0].apparent_wind_angle(false), Some(45.0));
        assert_eq!(t.samples[0].apparent_wind_angle(true), Some(71.565051177));
        t.samples[1].supplied_wind = Some((10.0, 270.0));
        t.samples[1].relate();
        assert_eq!(t.samples[1].apparent_wind_angle(false), Some(315.0));
        let f = SampleFilters {
            max_awa_change_deg: Some(89.0),
            ..none()
        };
        assert_eq!(filtered_out(&t, &f, false), [true; 3]);
        t.samples[1].downloaded_wind_only = true;
        t.samples[1].relate();
        assert_eq!(filtered_out(&t, &f, false), [false; 3]);
        t.samples[1].clear_env();
        assert_eq!(t.samples[1].wind_speed(), None);
        t.samples[1].downloaded_wind_only = false;
        t.samples[1].relate();
        assert_eq!(t.samples[1].wind_direction(), Some(270.0));
    }

    #[test]
    fn main_wave_ranges_use_inclusive_scene_precision_and_selected_bow() {
        let mut t = track(&[(0, Some(90.0), Some(6.0))]);
        let s = &mut t.samples[0];
        s.hs_m = Some(0.7);
        s.wave_from = Some(150.0);
        s.heading_corrected = Some(120.0);
        s.wave_period_s = Some(8.0);
        let mut ranges = pe_core::source::WaveRanges {
            height_m: Range {
                min: Some(0.7_f32 as f64),
                max: Some(0.7),
            },
            angle_deg: Range {
                min: Some(30.0),
                max: Some(30.0),
            },
            period_s: Range {
                min: Some(8.0),
                max: Some(8.0),
            },
        };
        assert!(!wave_ranges_exclude(s, &ranges, true));
        assert!(wave_ranges_exclude(s, &ranges, false));
        s.hs_m = None;
        assert!(wave_ranges_exclude(s, &ranges, true));
        ranges.height_m = Range::default();
        assert!(!wave_ranges_exclude(s, &ranges, true));
    }

    #[test]
    fn neighbour_changes_agree_with_an_independent_pointwise_reference() {
        let mut t = track(
            &(0..400)
                .map(|k| (k * 7, Some((k * 137 % 360) as f64), Some(6.0)))
                .collect::<Vec<_>>(),
        );
        // Include duplicate values, missing values and a gap between neighbouring observations.
        for k in 150..t.samples.len() {
            t.samples[k].t += 500;
        }
        for k in (0..t.samples.len()).step_by(13) {
            t.samples[k].heading = None;
        }
        t.derivation.max_gap_s = 60;
        for limit in [20.0, 137.0, 170.0, 180.0] {
            let f = SampleFilters {
                max_heading_change_deg: Some(limit),
                ..none()
            };
            let expected: Vec<_> = t
                .samples
                .iter()
                .enumerate()
                .map(|(i, a)| {
                    t.samples.iter().enumerate().any(|(j, b)| {
                        i.abs_diff(j) == 1
                            && (a.t - b.t).abs() <= t.derivation.max_gap_s
                            && a.heading.zip(b.heading).is_some_and(|(a, b)| {
                                let raw = (a - b).abs();
                                raw.min(360.0 - raw) > limit
                            })
                    })
                })
                .collect();
            assert_eq!(filtered_out(&t, &f, false), expected, "limit {limit}");
        }
    }

    #[test]
    fn the_default_filters_drop_slow_samples_and_manoeuvres() {
        let t = track(&[
            (0, Some(40.0), Some(6.0)),
            (60, Some(45.0), Some(6.0)),
            (120, Some(90.0), Some(5.0)),
            (180, Some(92.0), Some(0.5)),
            (240, Some(93.0), None),
        ]);
        let out = filtered_out(&t, &SampleFilters::default(), true);
        // 45° → 90° is a 45° turn, over the 30° default, which marks both
        // sides of it; 0.5 kn is under the 1 kn default; no speed cannot
        // show it passes a minimum.
        assert_eq!(out, [false, true, true, true, true]);
        assert_eq!(filtered_out(&t, &none(), true), [false; 5]);
    }

    /// A turn across a gap longer than the maximum gap (3 h by default) is
    /// not a manoeuvre: the boat was not seen turning.
    #[test]
    fn no_manoeuvre_across_a_gap() {
        let t = track(&[
            (0, Some(40.0), Some(6.0)),
            (60, Some(40.0), Some(6.0)),
            (60 + 4 * 3600, Some(200.0), Some(6.0)),
            (120 + 4 * 3600, Some(200.0), Some(6.0)),
        ]);
        assert_eq!(
            filtered_out(&t, &SampleFilters::default(), true),
            [false; 4]
        );
        let mut close = t.clone();
        close.derivation.max_gap_s = 5 * 3600;
        assert_eq!(
            filtered_out(&close, &SampleFilters::default(), true),
            [false, true, true, false]
        );
    }

    #[test]
    fn time_window_speed_band_and_origin() {
        let t = track(&[
            (0, None, Some(3.0)),
            (100, None, Some(6.0)),
            (200, None, Some(9.0)),
        ]);
        let window = SampleFilters {
            time_window: Some(TimeWindow {
                start: Some(50),
                end: Some(200),
            }),
            ..none()
        };
        assert_eq!(filtered_out(&t, &window, true), [true, false, false]);
        let band = SampleFilters {
            min_bsp_kn: Some(4.0),
            max_bsp_kn: Some(8.0),
            ..none()
        };
        assert_eq!(filtered_out(&t, &band, true), [true, false, true]);
    }

    /// Heading, course over the ground and wind direction are compass
    /// sectors clockwise from `from` to `to`, wrapping north; VMG is
    /// BSP × cos TWA. A sample without the value is out.
    #[test]
    fn heading_course_vmg_and_wind_direction_ranges() {
        let mut t = track(&[
            (0, Some(350.0), Some(6.0)),
            (60, Some(10.0), Some(6.0)),
            (120, Some(90.0), Some(6.0)),
            (180, None, Some(6.0)),
        ]);
        let sector = |from, to| Some(DirectionRange { from, to });
        let cog = SampleFilters {
            cog_deg: sector(340.0, 20.0),
            ..none()
        };
        assert_eq!(filtered_out(&t, &cog, false), [false, false, true, true]);
        // Through the water the second sample heads 30°: out of the sector.
        t.samples[1].heading_corrected = Some(30.0);
        let heading = SampleFilters {
            heading_deg: sector(340.0, 20.0),
            ..none()
        };
        assert_eq!(filtered_out(&t, &heading, true), [false, true, true, true]);
        assert_eq!(
            filtered_out(&t, &heading, false),
            [false, false, true, true]
        );
        // Wind from 180° at 10 kn: TWA 170, 170, 90 → VMG −5.909, −5.909, 0.
        for s in &mut t.samples {
            s.tws = Some(10.0);
            s.twd_from = Some(180.0);
            s.relate();
        }
        let downwind = SampleFilters {
            vmg_kn: Some(Range {
                min: Some(-6.0),
                max: Some(-5.0),
            }),
            ..none()
        };
        assert_eq!(
            filtered_out(&t, &downwind, false),
            [false, false, true, true]
        );
        let twd = SampleFilters {
            twd_deg: sector(170.0, 190.0),
            ..none()
        };
        assert_eq!(filtered_out(&t, &twd, false), [false, false, false, false]);
        t.samples[2].twd_from = Some(200.0);
        assert_eq!(filtered_out(&t, &twd, false), [false, false, true, false]);
        // Corrected for current, the wind direction through the water counts.
        t.samples[0].twd_from_corrected = Some(200.0);
        assert_eq!(filtered_out(&t, &twd, true), [true, false, true, false]);
    }

    #[test]
    fn environment_filters_need_the_environment() {
        let mut t = track(&[(0, Some(10.0), Some(6.0)), (60, Some(10.0), Some(6.0))]);
        let filters = SampleFilters {
            tws_kn: Some(Range {
                min: Some(8.0),
                max: Some(12.0),
            }),
            ..none()
        };
        assert_eq!(filtered_out(&t, &filters, true), [true, true]);
        t.samples[0].tws = Some(10.0);
        t.samples[0].twa = Some(50.0);
        assert_eq!(filtered_out(&t, &filters, true), [false, true]);
        assert_eq!(polar_point(&t.samples[0], true), Some((50.0, 10.0, 6.0)));
        assert_eq!(polar_point(&t.samples[1], true), None);

        // Corrected values win when asked for and present.
        t.samples[0].twa_corrected = Some(55.0);
        t.samples[0].tws_corrected = Some(11.0);
        t.samples[0].bsp_corrected = Some(6.5);
        assert_eq!(polar_point(&t.samples[0], true), Some((55.0, 11.0, 6.5)));
        assert_eq!(polar_point(&t.samples[0], false), Some((50.0, 10.0, 6.0)));

        // Waves: 100° off the bow is the beam; compass ranges wrap north.
        t.samples[0].wave_angle = Some(100.0);
        t.samples[0].wave_from = Some(350.0);
        let beam = SampleFilters {
            wave_direction: Some(WaveDirectionFilter::Sectors {
                sectors: vec![WaveSector::Beam],
            }),
            ..none()
        };
        assert_eq!(filtered_out(&t, &beam, true), [false, true]);
        let north = SampleFilters {
            wave_direction: Some(WaveDirectionFilter::Absolute {
                range: DirectionRange {
                    from: 300.0,
                    to: 20.0,
                },
            }),
            ..none()
        };
        assert_eq!(filtered_out(&t, &north, true), [false, true]);

        // A current from a tier without tides is dropped when asked.
        t.env_meta.datasets.push(DatasetRecord {
            name: "globcurrent".to_owned(),
            version: "1".to_owned(),
            fetched_at: 0,
            has_tide: Some(false),
        });
        t.samples[1].current_dataset = Some(0);
        let tides = SampleFilters {
            exclude_no_tide: true,
            ..none()
        };
        assert_eq!(filtered_out(&t, &tides, true), [false, true]);
    }

    #[test]
    fn sectors_and_compass_ranges() {
        assert_eq!(wave_sector(0.0), WaveSector::Head);
        assert_eq!(wave_sector(45.0), WaveSector::Bow);
        assert_eq!(wave_sector(90.0), WaveSector::Beam);
        assert_eq!(wave_sector(135.0), WaveSector::Quarter);
        assert_eq!(wave_sector(180.0), WaveSector::Following);
        assert!(in_compass_range(10.0, 300.0, 20.0));
        assert!(!in_compass_range(100.0, 300.0, 20.0));
        assert!(in_compass_range(100.0, 90.0, 110.0));
    }

    #[test]
    fn missing_environment_is_distinct_from_calm_environment() {
        let mut t = track(&[(0, Some(0.0), Some(6.0)); 3]);
        t.samples[0].hs_m = Some(0.0);
        t.samples[0].wave_from = Some(0.0);
        t.samples[0].current_speed = Some(0.0);
        t.samples[0].current_toward = Some(0.0);
        t.samples[1].hs_m = Some(1.0);
        t.samples[1].current_speed = Some(1.0);
        let f = SampleFilters {
            exclude_unknown_wave: true,
            exclude_unknown_current: true,
            ..none()
        };
        assert_eq!(filtered_out(&t, &f, true), [false, true, true]);
        assert_eq!(filtered_out(&t, &none(), true), [false; 3]);
    }

    /// A tack or gybe takes out the sample on either side of it and no
    /// more (settled with the user 2026-10-02); head to wind in between
    /// may bridge it; a gap longer than the track's maximum cannot.
    #[test]
    fn tacks_and_gybes_drop_only_the_samples_either_side() {
        let mut t = track(&[
            (0, Some(40.0), Some(6.0)),
            (60, Some(40.0), Some(6.0)),
            (120, Some(0.0), Some(6.0)),
            (180, Some(320.0), Some(6.0)),
            (240, Some(320.0), Some(6.0)),
            (300, Some(320.0), Some(6.0)),
        ]);
        for s in &mut t.samples {
            s.tws = Some(10.0);
            s.twd_from = Some(0.0);
            s.relate();
        }
        let f = SampleFilters {
            exclude_tacks: true,
            ..none()
        };
        assert_eq!(
            filtered_out(&t, &f, true),
            [false, true, false, true, false, false]
        );
        assert_eq!(
            filtered_out(&t, &none(), true),
            [false; 6],
            "off by default"
        );
        // Reverse the wind: the same ground turn is now a gybe.
        for s in &mut t.samples {
            s.twd_from = Some(180.0);
            s.relate();
        }
        assert_eq!(
            filtered_out(&t, &f, true),
            [false, true, false, true, false, false]
        );
        t.derivation.max_gap_s = 30;
        assert_eq!(filtered_out(&t, &f, true), [false; 6]);
    }

    #[test]
    fn wave_cog_filter_uses_the_ground_course_and_wraps_north() {
        let mut t = track(&[(0, Some(350.0), Some(6.0)), (60, None, Some(6.0))]);
        t.samples[0].wave_from = Some(10.0);
        t.samples[0].wave_angle = Some(90.0);
        t.samples[0].heading_corrected = Some(100.0);
        let f = SampleFilters {
            wave_direction: Some(WaveDirectionFilter::Cog {
                range: Range {
                    min: Some(15.0),
                    max: Some(25.0),
                },
            }),
            ..none()
        };
        assert_eq!(filtered_out(&t, &f, true), [false, true]);
    }
}
