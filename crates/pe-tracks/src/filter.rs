//! Sample filters (spec.md 7.6) and where a sample sits in the polar.
//!
//! A filter never deletes: it marks which samples are out, and the plots
//! draw those dimmed when asked. A sample lacking the value an active
//! filter reads (no wave height under a wave-height range, no speed under a
//! minimum boat speed) is out, since nothing shows it passes; with no
//! filter on a quantity, a missing value does not matter.

use pe_core::source::{OriginFilter, Range, SampleFilters, WaveDirectionFilter, WaveSector};
use pe_core::track::{Sample, Track, ValueOrigin};

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
    corrected.or_else(|| Some((sample.twa?, sample.tws?, sample.speed?)))
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

fn origin_passes(filter: OriginFilter, origin: Option<ValueOrigin>) -> bool {
    match filter {
        OriginFilter::Any => true,
        OriginFilter::GivenOnly => origin == Some(ValueOrigin::Given),
        OriginFilter::DerivedOnly => origin == Some(ValueOrigin::Derived),
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
        WaveDirectionFilter::Absolute { range } => sample
            .wave_from
            .is_some_and(|w| in_compass_range(w, range.from, range.to)),
    }
}

/// Which samples the filters take out, one flag per sample in order
/// (`true`: filtered out). Hand exclusions are separate (spec.md 10.3).
pub fn filtered_out(track: &Track, filters: &SampleFilters, use_corrected: bool) -> Vec<bool> {
    let samples = &track.samples;
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
                None => (sample.twa, sample.tws),
            };
            let bsp = boat_speed(sample, use_corrected);
            let passes = filters
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
                && filters
                    .max_heading_change_deg
                    .is_none_or(|limit| heading_change(samples, k, use_corrected) <= limit)
                && origin_passes(filters.heading_origin, sample.heading_origin)
                && origin_passes(filters.speed_origin, sample.speed_origin)
                && !(filters.exclude_no_tide && no_tide(sample.current_dataset));
            !passes
        })
        .collect()
}

/// The largest heading change between a sample and either neighbour,
/// degrees; 0 where a neighbour or the sample has no heading (a change
/// that cannot be seen is not a manoeuvre).
fn heading_change(samples: &[Sample], k: usize, use_corrected: bool) -> f64 {
    let Some(here) = heading(&samples[k], use_corrected) else {
        return 0.0;
    };
    [k.checked_sub(1), Some(k + 1)]
        .into_iter()
        .flatten()
        .filter_map(|j| samples.get(j))
        .filter_map(|s| heading(s, use_corrected))
        .map(|h| angle_between(here, h))
        .fold(0.0, f64::max)
}

#[cfg(test)]
mod tests {
    use pe_core::SampleId;
    use pe_core::TrackId;
    use pe_core::source::{DirectionRange, TimeWindow};
    use pe_core::track::{DatasetRecord, Fix, TrackOrigin};

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
        let given = SampleFilters {
            speed_origin: OriginFilter::DerivedOnly,
            ..none()
        };
        assert_eq!(filtered_out(&t, &given, true), [true; 3]);
        let heading = SampleFilters {
            heading_origin: OriginFilter::GivenOnly,
            ..none()
        };
        // No heading at all: not given.
        assert_eq!(filtered_out(&t, &heading, true), [true; 3]);
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
}
