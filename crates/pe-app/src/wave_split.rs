//! Split Wave Angle's blends (spec.md 10.5): the blend of each copy of the
//! 3D view, made from only the samples whose waves came from (or went to)
//! that copy's direction.
//!
//! The split is a way of looking, so nothing here is stored: a direction's
//! blend is the ordinary blend (`crate::blend::assemble`) over derived data
//! in which every track is binned again from its direction's samples alone.
//! Sources without waves — certificates and polar files — take part as they
//! are, so a copy shows what the boat's evidence says about those waves
//! against the same reference polars. The direction rule here and the
//! frontend's (`ui/src/polar/waveSplit.ts`, `waveBucket`) are the same rule
//! written twice, and pinned to the same numbers by their tests, so a copy's
//! dots and its blend never disagree about which samples are its own.

use std::collections::BTreeMap;
use std::sync::Arc;

use pe_core::source::SourceKind;
use pe_core::track::Sample;
use pe_core::{Project, Source};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

use crate::derived::{Derived, TrackDerived};
use crate::error::{AppError, Result};

/// Which direction a copy stands for: where its waves come from, or go to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "lowercase")]
#[ts(export_to = "WaveSense.ts")]
pub enum WaveSense {
    From,
    To,
}

/// How the view is split: into how many directions, in which sense.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, TS)]
#[ts(export_to = "WaveSplit.ts")]
pub struct WaveSplit {
    /// One of [`COUNTS`].
    pub count: u32,
    pub sense: WaveSense,
}

/// The counts the slider offers (spec.md 10.5).
pub const COUNTS: [u32; 6] = [4, 8, 16, 18, 24, 36];

impl WaveSplit {
    /// Refuses a count the slider does not offer: the frontend and Rust
    /// must agree on the directions, and an arbitrary count is a sign they
    /// do not.
    pub fn checked(self) -> Result<Self> {
        if COUNTS.contains(&self.count) {
            Ok(self)
        } else {
            Err(AppError::BadOption {
                field: "Wave directions",
                value: self.count.to_string(),
            })
        }
    }

    /// The copy a wave bearing (degrees clockwise from the bow) falls in,
    /// or `None` for an unknown bearing. Direction `k` is centred on
    /// `k × 360/count`, the first on the bow, so no boundary lies on 0°; a
    /// bearing on a boundary belongs to the direction clockwise of it.
    pub fn bucket(self, bearing: f32) -> Option<u32> {
        if !bearing.is_finite() {
            return None;
        }
        let width = 360.0 / self.count as f32;
        let direction = match self.sense {
            WaveSense::From => bearing,
            WaveSense::To => bearing + 180.0,
        };
        let turned = ((direction + width / 2.0) % 360.0 + 360.0) % 360.0;
        // A direction a rounding short of a full turn is the bow's, not one
        // past the last.
        Some(((turned / width).floor() as u32).min(self.count - 1))
    }

    /// Refuses a copy the split does not have.
    pub fn cell(self, cell: u32) -> Result<u32> {
        if cell < self.count {
            Ok(cell)
        } else {
            Err(AppError::BadOption {
                field: "Wave direction",
                value: format!("{cell} of {}", self.count),
            })
        }
    }
}

/// Where a sample's waves come from as seen from the boat: degrees
/// clockwise from the bow, 0–360, NaN when the waves or the heading are
/// unknown. The heading is the one the wave angle off the bow is measured
/// from: through the water when the project corrects for current.
pub fn sample_bearing(project: &Project, sample: &Sample) -> f32 {
    let heading = if project.blend.use_corrected {
        sample.heading_corrected.or(sample.heading)
    } else {
        sample.heading
    };
    sample
        .wave_from
        .zip(heading)
        .map_or(f32::NAN, |(w, h)| bearing(w, h))
}

/// The wave bearing from a compass wave direction and heading. Unlike the
/// wave angle off the bow (`pe_tracks::geo::angle_between`) it keeps its
/// side: 30 for waves 30° to starboard of the bow, 330 for 30° to port.
pub fn bearing(wave_from: f64, heading: f64) -> f32 {
    let bearing = pe_tracks::geo::wrap_360(wave_from - heading) as f32;
    // f32 can round a value a hair under 360 up to 360.0 itself.
    if bearing >= 360.0 { 0.0 } else { bearing }
}

/// Every visible source's derived data as copy `cell` reads it: each track
/// binned again from the samples of that direction (after its own filters,
/// the global filters and the wave ranges, which `derived` already holds),
/// every other source as it is.
///
/// A direction's track data serves the blend alone, so its sample points
/// and filter flags, which only the scene reads, are left empty rather
/// than copied once per direction.
pub fn direction_derived(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    split: WaveSplit,
    cell: u32,
) -> BTreeMap<u64, Arc<Derived>> {
    derived
        .iter()
        .map(|(id, data)| {
            let rebinned = project
                .sources
                .iter()
                .find(|source| source.id.raw() == *id)
                .and_then(|source| rebin(project, source, data, split, cell));
            (*id, rebinned.unwrap_or_else(|| Arc::clone(data)))
        })
        .collect()
}

fn rebin(
    project: &Project,
    source: &Source,
    data: &Derived,
    split: WaveSplit,
    cell: u32,
) -> Option<Arc<Derived>> {
    let SourceKind::Track { track } = &source.kind else {
        return None;
    };
    let placed = data.track.as_ref()?;
    let flags: Vec<bool> = track
        .samples
        .iter()
        .zip(&placed.filtered)
        .map(|(sample, out)| *out || split.bucket(sample_bearing(project, sample)) != Some(cell))
        .collect();
    let (base, track) = crate::derived::derive_base_with_filters(project, source, Some(&flags));
    let edited = pe_polar::with_overlay(base.clone(), &source.overlay, false);
    let blend = pe_polar::with_overlay(edited.clone(), &source.overlay, true);
    Some(Arc::new(Derived {
        base,
        track: track.map(|placed| {
            Arc::new(TrackDerived {
                points: Vec::new(),
                filtered: Vec::new(),
                segment: placed.segment.clone(),
            })
        }),
        edited,
        blend,
    }))
}

/// The blend of every copy, in copy order; `None` where the copy's blend
/// has nothing to say off the 0° row (no sample of that direction, and no
/// other source).
pub fn blends(
    project: &Project,
    derived: &BTreeMap<u64, Arc<Derived>>,
    split: WaveSplit,
) -> Vec<Option<pe_polar::Blend>> {
    (0..split.count)
        .map(|cell| {
            let blend =
                crate::blend::assemble(project, &direction_derived(project, derived, split, cell));
            pe_polar::blend::has_value_off_zero_row(&blend.polar).then_some(blend)
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use pe_core::track::{Fix, Track, TrackOrigin};
    use pe_core::{Boat, Colour, SampleId, SourceId, TrackId};

    use super::*;
    use crate::derived::Derivations;

    const FOUR: WaveSplit = WaveSplit {
        count: 4,
        sense: WaveSense::From,
    };
    const EIGHT: WaveSplit = WaveSplit {
        count: 8,
        sense: WaveSense::From,
    };

    #[test]
    fn directions_are_centred_on_the_bow_and_a_boundary_belongs_clockwise() {
        // The same numbers as ui/src/polar/waveSplit.test.ts.
        assert_eq!(EIGHT.bucket(0.0), Some(0));
        assert_eq!(EIGHT.bucket(22.4), Some(0));
        assert_eq!(EIGHT.bucket(22.5), Some(1));
        assert_eq!(EIGHT.bucket(337.5), Some(0));
        assert_eq!(EIGHT.bucket(337.4), Some(7));
        assert_eq!(EIGHT.bucket(359.9999), Some(0));
        assert_eq!(FOUR.bucket(45.0), Some(1));
        assert_eq!(FOUR.bucket(314.9), Some(3));
        assert_eq!(FOUR.bucket(315.0), Some(0));
        assert_eq!(EIGHT.bucket(f32::NAN), None);
        let to = WaveSplit {
            count: 8,
            sense: WaveSense::To,
        };
        assert_eq!(to.bucket(10.0), Some(4), "waves from 10° go to 190°");
        assert_eq!(to.bucket(200.0), Some(0));
    }

    #[test]
    fn only_the_slider_counts_and_the_split_cells_are_accepted() {
        assert!(FOUR.checked().is_ok());
        for count in [0, 1, 5, 12, 360] {
            let split = WaveSplit { count, ..FOUR };
            assert!(split.checked().is_err(), "{count}");
        }
        assert_eq!(FOUR.cell(3).unwrap(), 3);
        assert!(FOUR.cell(4).is_err());
    }

    #[test]
    fn the_bearing_keeps_its_side_and_follows_the_corrected_heading() {
        assert_eq!(bearing(20.0, 350.0), 30.0);
        assert_eq!(bearing(350.0, 20.0), 330.0);
        assert_eq!(bearing(359.999_999_999_999_9, 0.0), 0.0);
        let mut project = Project::new("P", Boat::default(), 0);
        let fix = Fix {
            tws: None,
            twd_from: None,
            t: 0,
            lat: 0.0,
            lon: 0.0,
            cog: None,
            sog: None,
        };
        let mut sample = Sample::at(SampleId(1), 0, &fix);
        assert!(sample_bearing(&project, &sample).is_nan(), "no waves");
        sample.wave_from = Some(100.0);
        sample.heading = Some(90.0);
        sample.heading_corrected = Some(80.0);
        project.blend.use_corrected = false;
        assert_eq!(sample_bearing(&project, &sample), 10.0);
        project.blend.use_corrected = true;
        assert_eq!(sample_bearing(&project, &sample), 20.0);
        sample.heading = None;
        sample.heading_corrected = None;
        assert!(sample_bearing(&project, &sample).is_nan(), "no heading");
    }

    /// Ten samples at 90° in 12 kn, BSP 7.00–7.09, heading north: the
    /// first five with waves on the bow, the rest on the starboard beam.
    fn project() -> Project {
        let mut project = Project::new("P", Boat::default(), 0);
        let mut track = Track::new(
            TrackId(1000),
            TrackOrigin::File {
                name: "t.csv".to_owned(),
                boat_name: None,
            },
        );
        for k in 0..10u64 {
            let fix = Fix {
                tws: None,
                twd_from: None,
                t: k as i64 * 60,
                lat: 0.0,
                lon: 0.0,
                cog: None,
                sog: None,
            };
            let mut sample = Sample::at(SampleId(k), k as u32, &fix);
            sample.tws = Some(12.0);
            sample.twa = Some(90.0);
            sample.speed = Some(7.0 + k as f64 / 100.0);
            sample.heading = Some(0.0);
            sample.wave_from = Some(if k < 5 { 0.0 } else { 90.0 });
            track.samples.push(sample);
            track.fixes.push(fix);
        }
        let mut source = Source::new(
            SourceId(2),
            "t",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(track),
            },
        );
        source.overlay.filters.min_bsp_kn = None;
        source.overlay.filters.max_heading_change_deg = None;
        project.sources = vec![source];
        project.blend.use_corrected = false;
        project.next_id = 100_000;
        project
    }

    #[test]
    fn each_direction_blends_its_own_samples_and_an_empty_direction_has_no_blend() {
        let project = project();
        let mut derivations = Derivations::default();
        let derived = derivations.visible(&project);
        let i = project.grid.twa.iter().position(|v| *v == 90.0).unwrap();
        let j = project.grid.tws.iter().position(|v| *v == 12.0).unwrap();
        let blends = blends(&project, &derived, FOUR);
        assert_eq!(blends.len(), 4);
        // The 90th percentile of 7.00–7.04: rank 3.6 → 7.036; of 7.05–7.09: 7.086.
        let bow = blends[0].as_ref().unwrap().polar.get(i, j).unwrap();
        assert!((bow - 7.036).abs() < 1e-9, "{bow}");
        let beam = blends[1].as_ref().unwrap().polar.get(i, j).unwrap();
        assert!((beam - 7.086).abs() < 1e-9, "{beam}");
        assert!(blends[2].is_none(), "no waves from astern");
        assert!(blends[3].is_none(), "none from port");
        // To: the same samples, half a turn round.
        let to = blends_to(&project, &derived);
        assert!(to[0].is_none() && to[1].is_none());
        assert!((to[2].as_ref().unwrap().polar.get(i, j).unwrap() - 7.036).abs() < 1e-9);
        assert!((to[3].as_ref().unwrap().polar.get(i, j).unwrap() - 7.086).abs() < 1e-9);
        // The whole blend is untouched: all ten, rank 8.1 → 7.081.
        let whole = derivations.blend(&project).polar.get(i, j).unwrap();
        assert!((whole - 7.081).abs() < 1e-9, "{whole}");
    }

    fn blends_to(
        project: &Project,
        derived: &BTreeMap<u64, Arc<Derived>>,
    ) -> Vec<Option<pe_polar::Blend>> {
        blends(
            project,
            derived,
            WaveSplit {
                count: 4,
                sense: WaveSense::To,
            },
        )
    }

    #[test]
    fn a_direction_keeps_the_filters_and_exclusions_the_view_has() {
        let mut project = project();
        // The 7.04 sample is excluded; with four left on the bow, the
        // five-sample floor leaves that direction with no segment.
        project.sources[0].overlay.excluded_samples = vec![SampleId(4)];
        let mut derivations = Derivations::default();
        let derived = derivations.visible(&project);
        let blends = blends(&project, &derived, FOUR);
        assert!(blends[0].is_none());
        assert!(blends[1].is_some());
    }

    #[test]
    fn the_split_blends_are_cached_until_the_blend_itself_would_move() {
        let mut project = project();
        let mut derivations = Derivations::default();
        let first = derivations.split_blends(&project, FOUR);
        assert!(Arc::ptr_eq(
            &first,
            &derivations.split_blends(&project, FOUR)
        ));
        assert!(!Arc::ptr_eq(
            &first,
            &derivations.split_blends(&project, EIGHT)
        ));
        assert_eq!(derivations.split_blends(&project, FOUR).len(), 4);
        project.blend.smoothing = true;
        assert!(!Arc::ptr_eq(
            &first,
            &derivations.split_blends(&project, FOUR)
        ));
    }
}
