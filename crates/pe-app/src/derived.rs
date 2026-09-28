//! Derived data per source, recomputed only when that source changes
//! (spec.md 10.4, 13; plan.md M13).
//!
//! Every view reads a source through what is derived from it: a polar
//! source's grid with its edits, a track's samples placed in the polar with
//! their filter flags, and its polar segment (spec.md 12.1). Deriving a
//! segment reads every sample of the track, so doing it for every source on
//! every edit would spend the edit-to-view budget (spec.md 13) on sources
//! that did not change. Instead each source carries revisions, bumped only by
//! the changes that reach it, and the cache recomputes an entry only when its
//! revisions moved.
//!
//! **Never persisted** (invariant 2): the cache and the revisions live in the
//! open project's session state and start empty on every opening. Anything
//! that changes a project outside a command bumps everything
//! ([`Derivations::invalidate_all`]) unless it names the source it wrote
//! ([`Derivations::samples_changed`]), so a missed case costs time, never a
//! stale answer.

use std::collections::BTreeMap;
use std::sync::Arc;

use pe_core::source::{Source, SourceKind};
use pe_core::track::SegmentStatistic;
use pe_core::{Command, Project};
use pe_polar::{Polar, Segment};

/// How far a change reaches into what is derived from one source. Each
/// level includes the ones below it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Touch {
    /// Its grid as edited: overrides and excluded nodes.
    Polar,
    /// A track's segment: which samples count (filters, exclusions) and how
    /// a cell sums them up.
    Segment,
    /// A track's samples themselves: where each sits in the polar.
    Samples,
}

/// What one command changes, per source; `all` when it reaches every
/// source (the choice of corrected or ground values moves every sample).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Touches {
    /// Source id and how far.
    pub sources: Vec<(u64, Touch)>,
    /// Everything derived is stale.
    pub all: bool,
}

/// What `command` changes. The match has no wildcard, so a new command does
/// not compile until it says what it touches.
pub fn touches(command: &Command) -> Touches {
    let mut out = Touches::default();
    collect(command, &mut out);
    out
}

fn collect(command: &Command, out: &mut Touches) {
    let mut one = |id: pe_core::SourceId, touch| out.sources.push((id.raw(), touch));
    match command {
        Command::AddSource { source, .. } | Command::RemoveSource { source, .. } => {
            one(source.id, Touch::Samples);
        }
        Command::SetDerivation { source, .. } => one(*source, Touch::Samples),
        Command::ExcludeSamples { source, .. }
        | Command::IncludeSamples { source, .. }
        | Command::SetSampleFilters { source, .. }
        | Command::SetSegmentStatistic { source, .. } => one(*source, Touch::Segment),
        Command::ExcludeCells { source, .. }
        | Command::IncludeCells { source, .. }
        | Command::EditCells { source, .. } => one(*source, Touch::Polar),
        Command::SetUseCorrected { .. } => out.all = true,
        // Colour, label, weight, visibility and order change no source's
        // own derived data; the views read them from the project directly.
        Command::RenameProject { .. }
        | Command::SetSourceColour { .. }
        | Command::SetSourceVisible { .. }
        | Command::SetSourceWeight { .. }
        | Command::SetSourceLabel { .. }
        | Command::MoveSource { .. }
        | Command::SetStokesDrift { .. } => {}
        Command::Batch { commands, .. } => {
            for command in commands {
                collect(command, out);
            }
        }
    }
}

/// A source's revisions: each moves when a change reaches that level.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SourceRevs {
    /// Moves with [`Touch::Samples`].
    pub samples: u64,
    /// Moves with [`Touch::Segment`] and above.
    pub segment: u64,
    /// Moves with every touch.
    pub polar: u64,
}

/// A track's samples as the polar views place them, and its segment.
#[derive(Debug, Clone, PartialEq)]
pub struct TrackDerived {
    /// Each sample's (TWA °, TWS kn, BSP kn), `None` without wind, in sample
    /// order (spec.md 7.5: water-relative where the project says so).
    pub points: Vec<Option<(f64, f64, f64)>>,
    /// Whether the filters take each sample out, in sample order.
    pub filtered: Vec<bool>,
    /// The polar segment on the output grid (spec.md 12.1).
    pub segment: Segment,
}

/// What one source's views read.
#[derive(Debug, Clone, PartialEq)]
pub struct Derived {
    /// The editable surface before edits: the imported or VPP grid, or the
    /// track's segment (spec.md 10.4).
    pub base: Polar,
    /// Samples and segment, for a track.
    pub track: Option<Arc<TrackDerived>>,
    /// `base` with the edits written in: what the 3D view and the table show.
    pub edited: Polar,
    /// `edited` with excluded nodes emptied: what the blend reads
    /// (spec.md 12.3) and the 2D curves draw.
    pub blend: Polar,
}

/// What a track's derived data depends on besides its own revisions.
#[derive(Debug, Clone, PartialEq)]
struct TrackKey {
    epoch: u64,
    samples: u64,
    segment: u64,
    use_corrected: bool,
    min_samples: u32,
    statistic: SegmentStatistic,
    twa: Vec<f64>,
    tws: Vec<f64>,
}

#[derive(Debug, Clone)]
struct Entry {
    base_key: Option<TrackKey>,
    base_epoch: u64,
    edit_key: (u64, u64),
    value: Arc<Derived>,
}

/// The revisions and the cache of one opening of a project.
#[derive(Debug, Default)]
pub struct Derivations {
    counter: u64,
    /// Moves when everything is stale.
    epoch: u64,
    revs: BTreeMap<u64, SourceRevs>,
    cache: BTreeMap<u64, Entry>,
    /// Entries computed since the opening, for tests of what recomputes.
    pub computed: u64,
}

impl Derivations {
    fn next(&mut self) -> u64 {
        self.counter += 1;
        self.counter
    }

    /// Records a change that reached `id` as far as `touch`.
    pub fn bump(&mut self, id: u64, touch: Touch) {
        let rev = self.next();
        let revs = self.revs.entry(id).or_default();
        revs.polar = rev;
        if touch >= Touch::Segment {
            revs.segment = rev;
        }
        if touch >= Touch::Samples {
            revs.samples = rev;
        }
    }

    /// Records what a command changed.
    pub fn record(&mut self, touches: &Touches) {
        if touches.all {
            self.invalidate_all();
        }
        for (id, touch) in &touches.sources {
            self.bump(*id, *touch);
        }
    }

    /// Everything derived is stale: a change the caller cannot place.
    pub fn invalidate_all(&mut self) {
        self.epoch = self.next();
    }

    /// A track's samples were written outside a command (the environment
    /// fetch).
    pub fn samples_changed(&mut self, id: u64) {
        self.bump(id, Touch::Samples);
    }

    /// The revisions of `id`.
    pub fn revs(&self, id: u64) -> SourceRevs {
        self.revs.get(&id).copied().unwrap_or_default()
    }

    /// Names the samples section of a 3D scene: equal keys mean the same
    /// visible sources in the same order, each track's samples placed as
    /// before, so only their flags can differ. FNV-1a over plain numbers:
    /// nothing here iterates a hash map. Kept to 53 bits, so the frontend
    /// holds it exactly as a number and names it back.
    pub fn samples_key(&self, project: &Project) -> u64 {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        let mut mix = |value: u64| {
            for byte in value.to_le_bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0100_0000_01b3);
            }
        };
        mix(self.epoch);
        mix(u64::from(project.blend.use_corrected));
        for source in project.sources.iter().filter(|s| s.visible) {
            mix(source.id.raw());
            if source.track().is_some() {
                mix(self.revs(source.id.raw()).samples);
            }
        }
        hash & ((1 << 53) - 1)
    }

    /// What `source` derives to, from the cache when nothing it depends on
    /// has moved.
    pub fn get(&mut self, project: &Project, source: &Source) -> Arc<Derived> {
        let id = source.id.raw();
        let revs = self.revs(id);
        let base_key = source.track().map(|track| TrackKey {
            epoch: self.epoch,
            samples: revs.samples,
            segment: revs.segment,
            use_corrected: project.blend.use_corrected,
            min_samples: project.blend.min_samples,
            statistic: track.statistic,
            twa: project.grid.twa.clone(),
            tws: project.grid.tws.clone(),
        });
        let edit_key = (self.epoch, revs.polar);
        let cached = self.cache.get(&id);
        let base_fresh = cached
            .is_some_and(|entry| entry.base_key == base_key && entry.base_epoch == self.epoch);
        if let Some(entry) = cached
            && base_fresh
            && entry.edit_key == edit_key
        {
            return Arc::clone(&entry.value);
        }
        self.computed += 1;
        let (base, track) = match cached {
            Some(entry) if base_fresh => (entry.value.base.clone(), entry.value.track.clone()),
            _ => derive_base(project, source),
        };
        let edited = pe_polar::with_overlay(base.clone(), &source.overlay, false);
        let blend = pe_polar::with_overlay(edited.clone(), &source.overlay, true);
        let value = Arc::new(Derived {
            base,
            track,
            edited,
            blend,
        });
        self.cache.insert(
            id,
            Entry {
                base_key,
                base_epoch: self.epoch,
                edit_key,
                value: Arc::clone(&value),
            },
        );
        value
    }

    /// Every visible source's derived data, by id.
    pub fn visible(&mut self, project: &Project) -> BTreeMap<u64, Arc<Derived>> {
        project
            .sources
            .iter()
            .filter(|s| s.visible)
            .map(|s| (s.id.raw(), self.get(project, s)))
            .collect()
    }
}

/// The editable surface of a source before edits, and a track's samples.
fn derive_base(project: &Project, source: &Source) -> (Polar, Option<Arc<TrackDerived>>) {
    match &source.kind {
        SourceKind::Orc { .. } | SourceKind::PolarFile { .. } => {
            (pe_polar::source_polar(source).unwrap_or_default(), None)
        }
        SourceKind::Track { track } => {
            let use_corrected = project.blend.use_corrected;
            let points: Vec<_> = track
                .samples
                .iter()
                .map(|s| pe_tracks::polar_point(s, use_corrected))
                .collect();
            let filtered = pe_tracks::filtered_out(track, &source.overlay.filters, use_corrected);
            let excluded = &source.overlay.excluded_samples;
            // Samples that pass the filters and are not excluded (spec.md
            // 12.1).
            let used = track
                .samples
                .iter()
                .zip(&points)
                .zip(&filtered)
                .filter(|((sample, _), out)| !**out && excluded.binary_search(&sample.id).is_err())
                .filter_map(|((_, point), _)| *point);
            let segment = pe_polar::bin(
                used,
                &project.grid.twa,
                &project.grid.tws,
                track.statistic,
                project.blend.min_samples,
            );
            let base = segment.polar.clone();
            (
                base,
                Some(Arc::new(TrackDerived {
                    points,
                    filtered,
                    segment,
                })),
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use pe_core::command::{CellEdit, EditAction};
    use pe_core::polar::{PolarFileFormat, PolarGrid};
    use pe_core::track::{Sample, Track, TrackOrigin};
    use pe_core::{Boat, Colour, SampleId, SourceId, TrackId};

    use super::*;

    fn file(id: u64) -> Source {
        Source::new(
            SourceId(id),
            "a.pol",
            Colour::parse("#4e79a7").unwrap(),
            SourceKind::PolarFile {
                format: PolarFileFormat::Adrena,
                file_name: "a.pol".to_owned(),
                polar: PolarGrid {
                    twa: vec![52.0, 90.0],
                    tws: vec![6.0, 12.0],
                    bsp: vec![vec![Some(5.0), Some(6.5)], vec![Some(6.0), Some(8.0)]],
                },
            },
        )
    }

    /// A track of `n` samples at 90° in 12 kn, BSP 7 + k/100.
    fn track(id: u64, n: u64) -> Source {
        let mut track = Track::new(
            TrackId(id + 1000),
            TrackOrigin::File {
                name: "t.csv".to_owned(),
                boat_name: None,
            },
        );
        for k in 0..n {
            let fix = pe_core::track::Fix {
                t: k as i64 * 60,
                lat: 0.0,
                lon: 0.0,
                cog: None,
                sog: None,
            };
            let mut sample = Sample::at(SampleId(id * 10_000 + k), k as u32, &fix);
            sample.tws = Some(12.0);
            sample.twa = Some(90.0);
            sample.speed = Some(7.0 + k as f64 / 100.0);
            track.samples.push(sample);
            track.fixes.push(fix);
        }
        let mut source = Source::new(
            SourceId(id),
            "t",
            Colour::parse("#e15759").unwrap(),
            SourceKind::Track {
                track: Box::new(track),
            },
        );
        // No filters: every sample counts.
        source.overlay.filters.min_bsp_kn = None;
        source.overlay.filters.max_heading_change_deg = None;
        source
    }

    fn project() -> Project {
        let mut project = Project::new("P", Boat::default(), 0);
        project.sources = vec![file(1), track(2, 10)];
        project.next_id = 100_000;
        project
    }

    #[test]
    fn a_track_segment_is_binned_from_its_samples() {
        let project = project();
        let mut derivations = Derivations::default();
        let derived = derivations.get(&project, &project.sources[1]);
        let segment = &derived.track.as_ref().unwrap().segment;
        let i = project.grid.twa.iter().position(|v| *v == 90.0).unwrap();
        let j = project.grid.tws.iter().position(|v| *v == 12.0).unwrap();
        assert_eq!(segment.count[i][j], 10);
        // The 90th percentile of 7.00..7.09: rank 8.1 → 7.081.
        let value = derived.base.bsp[i][j].unwrap();
        assert!((value - 7.081).abs() < 1e-9, "{value}");
        assert_eq!(derived.edited, derived.base);
    }

    /// An edit to one source recomputes only that source; the track's
    /// segment is not binned again for a polar edit, nor the polar for a
    /// track's exclusion.
    #[test]
    fn a_change_recomputes_only_the_source_it_reaches() {
        let mut project = project();
        let mut derivations = Derivations::default();
        let (file_source, track_source) = (project.sources[0].clone(), project.sources[1].clone());
        let first_track = derivations.get(&project, &track_source);
        derivations.get(&project, &file_source);
        assert_eq!(derivations.computed, 2);
        derivations.get(&project, &track_source);
        assert_eq!(derivations.computed, 2, "nothing moved");

        let mut edit = Command::EditCells {
            source: SourceId(1),
            action: EditAction::Type,
            cells: vec![CellEdit {
                twa: 90.0,
                tws: 12.0,
                before: None,
                after: Some(8.4),
            }],
        };
        edit.apply(&mut project).unwrap();
        derivations.record(&touches(&edit));
        let file_after = derivations.get(&project, &project.sources[0].clone());
        assert_eq!(file_after.edited.bsp[1][1], Some(8.4));
        let track_again = derivations.get(&project, &track_source);
        assert_eq!(derivations.computed, 3, "only the edited source");
        assert!(Arc::ptr_eq(&first_track, &track_again));

        // An override on the track's segment keeps its binned samples.
        let mut track_edit = Command::EditCells {
            source: SourceId(2),
            action: EditAction::Type,
            cells: vec![CellEdit {
                twa: 90.0,
                tws: 12.0,
                before: None,
                after: Some(9.0),
            }],
        };
        track_edit.apply(&mut project).unwrap();
        derivations.record(&touches(&track_edit));
        let edited_track = derivations.get(&project, &project.sources[1].clone());
        assert!(Arc::ptr_eq(
            edited_track.track.as_ref().unwrap(),
            first_track.track.as_ref().unwrap()
        ));
        let i = project.grid.twa.iter().position(|v| *v == 90.0).unwrap();
        let j = project.grid.tws.iter().position(|v| *v == 12.0).unwrap();
        assert_eq!(edited_track.edited.bsp[i][j], Some(9.0));
        assert_ne!(edited_track.base.bsp[i][j], Some(9.0));

        // Excluding samples re-bins the track.
        let ids: Vec<SampleId> = project.sources[1]
            .track()
            .unwrap()
            .samples
            .iter()
            .take(6)
            .map(|s| s.id)
            .collect();
        let mut exclude = Command::ExcludeSamples {
            source: SourceId(2),
            samples: ids,
        };
        exclude.apply(&mut project).unwrap();
        derivations.record(&touches(&exclude));
        let fewer = derivations.get(&project, &project.sources[1].clone());
        assert_eq!(fewer.track.as_ref().unwrap().segment.count[i][j], 4);
        // Four samples are below the minimum of five: the cell is empty in
        // the segment, and the override still holds it.
        assert_eq!(fewer.base.bsp[i][j], None);
        assert_eq!(fewer.edited.bsp[i][j], Some(9.0));
    }

    /// The samples key moves when samples move or the visible list changes,
    /// and not for an edit or an exclusion.
    #[test]
    fn the_samples_key_follows_sample_positions_only() {
        let mut project = project();
        let mut derivations = Derivations::default();
        let key = derivations.samples_key(&project);
        derivations.bump(2, Touch::Segment);
        derivations.bump(1, Touch::Polar);
        assert_eq!(derivations.samples_key(&project), key);
        derivations.samples_changed(2);
        let moved = derivations.samples_key(&project);
        assert_ne!(moved, key);
        project.sources[0].visible = false;
        assert_ne!(derivations.samples_key(&project), moved);
        project.sources[0].visible = true;
        derivations.invalidate_all();
        assert_ne!(derivations.samples_key(&project), moved);
    }
}
