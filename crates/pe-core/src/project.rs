//! The project: one attempt to build one polar for one boat (spec.md 2, 4.1).

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::canonical;
use crate::error::{CoreError, Result};
use crate::id::{ProjectId, SampleId, SourceId, TrackId};
use crate::polar::validate_axis;
use crate::source::{Colour, PALETTE, Source};

/// The document schema version this build writes.
///
/// Opening a newer version is refused; older versions migrate forward on open
/// (`io::MIGRATIONS`).
pub const SCHEMA_VERSION: u32 = 1;

/// The boat the polar is for. Free text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Boat {
    /// Boat name.
    pub name: String,
    /// Notes.
    pub notes: String,
}

/// The output grid every source is resampled onto (spec.md 12.2).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutputGrid {
    /// True wind angles, degrees in [0, 180], strictly increasing.
    #[serde(with = "canonical::degrees_list")]
    pub twa: Vec<f64>,
    /// True wind speeds, knots, strictly increasing.
    #[serde(with = "canonical::knots_list")]
    pub tws: Vec<f64>,
}

impl Default for OutputGrid {
    fn default() -> Self {
        Self {
            twa: vec![
                0.0, 30.0, 35.0, 40.0, 45.0, 52.0, 60.0, 70.0, 75.0, 80.0, 90.0, 100.0, 110.0,
                120.0, 135.0, 150.0, 160.0, 170.0, 180.0,
            ],
            tws: vec![4.0, 6.0, 8.0, 10.0, 12.0, 14.0, 16.0, 20.0, 25.0, 30.0],
        }
    }
}

/// How the blend is computed and shown (spec.md 7.5, 8, 12).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct BlendSettings {
    /// Samples a track cell needs before it has a value (spec.md 12.1).
    pub min_samples: u32,
    /// Samples at which a track cell reaches full confidence (spec.md 12.3).
    pub n_full: u32,
    /// Smooth the filled grid (off by default).
    pub smoothing: bool,
    /// Feed the polar from current-corrected values where a current exists
    /// (spec.md 7.5, D13).
    pub use_corrected: bool,
    /// Include Stokes drift in the global merged current (spec.md 7.5.1).
    pub include_stokes_drift: bool,
    /// The blend's colour in every plot.
    pub colour: Colour,
    /// Whether the blend is drawn.
    pub visible: bool,
}

impl Default for BlendSettings {
    fn default() -> Self {
        Self {
            min_samples: 5,
            n_full: 30,
            smoothing: false,
            use_corrected: true,
            include_stokes_drift: false,
            colour: Colour::trusted("#ffffff"),
            visible: true,
        }
    }
}

/// A whole project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Project {
    /// The schema this document follows.
    pub schema_version: u32,
    /// Identity, independent of the file name.
    pub id: ProjectId,
    /// Display name.
    pub name: String,
    /// When it was created, UTC epoch seconds.
    pub created: i64,
    /// The boat.
    #[serde(default)]
    pub boat: Boat,
    /// The output grid.
    #[serde(default)]
    pub grid: OutputGrid,
    /// Blend settings.
    #[serde(default)]
    pub blend: BlendSettings,
    /// Every source, in the order the user sees them.
    #[serde(default)]
    pub sources: Vec<Source>,
    /// The next id to allocate. Ids are never reused.
    pub next_id: u64,
}

impl Project {
    /// A new, empty project with default grid and blend settings.
    pub fn new(name: impl Into<String>, boat: Boat, created: i64) -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            id: ProjectId::fresh(),
            name: name.into(),
            created,
            boat,
            grid: OutputGrid::default(),
            blend: BlendSettings::default(),
            sources: Vec::new(),
            next_id: 1,
        }
    }

    fn allocate(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id = self.next_id.saturating_add(1);
        id
    }

    /// A source id no source has had.
    pub fn allocate_source_id(&mut self) -> SourceId {
        SourceId(self.allocate())
    }

    /// A track id no track has had.
    pub fn allocate_track_id(&mut self) -> TrackId {
        TrackId(self.allocate())
    }

    /// A sample id no sample has had.
    pub fn allocate_sample_id(&mut self) -> SampleId {
        SampleId(self.allocate())
    }

    /// The source with this id.
    pub fn source(&self, id: SourceId) -> Option<&Source> {
        self.sources.iter().find(|s| s.id == id)
    }

    /// The source with this id, mutably.
    pub fn source_mut(&mut self, id: SourceId) -> Option<&mut Source> {
        self.sources.iter_mut().find(|s| s.id == id)
    }

    /// Where the source with this id sits in the list.
    pub fn source_index(&self, id: SourceId) -> Option<usize> {
        self.sources.iter().position(|s| s.id == id)
    }

    /// The first palette colour no source uses, or the palette continued by
    /// count when all sixteen are taken (spec.md 8).
    pub fn next_palette_colour(&self) -> Colour {
        let used: BTreeSet<&str> = self.sources.iter().map(|s| s.colour.as_str()).collect();
        let pick = PALETTE
            .iter()
            .find(|c| !used.contains(**c))
            .copied()
            .unwrap_or(PALETTE[self.sources.len() % PALETTE.len()]);
        Colour::trusted(pick)
    }

    /// Checks every rule the document must hold before it is written.
    pub fn validate(&self) -> Result<()> {
        if self.name.trim().is_empty() {
            return Err(CoreError::Invalid("the project has no name".to_owned()));
        }
        validate_axis(&self.grid.twa, "output TWA", 0.0, 180.0)?;
        validate_axis(&self.grid.tws, "output TWS", 0.0, f64::MAX)?;

        let mut ids = BTreeSet::new();
        let mut claim = |raw: u64, what: &str| -> Result<()> {
            if raw >= self.next_id {
                return Err(CoreError::Invalid(format!(
                    "{what} #{raw} is not below the next id {}",
                    self.next_id
                )));
            }
            if !ids.insert(raw) {
                return Err(CoreError::Invalid(format!("id #{raw} is used twice")));
            }
            Ok(())
        };
        for source in &self.sources {
            claim(source.id.raw(), "source")?;
            source.validate()?;
            if let Some(track) = source.track() {
                claim(track.id.raw(), "track")?;
                for sample in &track.samples {
                    claim(sample.id.raw(), "sample")?;
                    if sample.fix as usize >= track.fixes.len() {
                        return Err(CoreError::Invalid(format!(
                            "sample #{} refers to fix {} of {}",
                            sample.id.raw(),
                            sample.fix,
                            track.fixes.len()
                        )));
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::polar::{PolarFileFormat, PolarGrid};
    use crate::source::SourceKind;

    fn polar_source(project: &mut Project) -> Source {
        let id = project.allocate_source_id();
        let colour = project.next_palette_colour();
        Source::new(
            id,
            "file",
            colour,
            SourceKind::PolarFile {
                format: PolarFileFormat::Expedition,
                file_name: "a.txt".to_owned(),
                polar: PolarGrid::empty(vec![45.0], vec![10.0]),
            },
        )
    }

    #[test]
    fn a_new_project_has_the_spec_defaults() {
        let p = Project::new("Fastnet", Boat::default(), 0);
        assert_eq!(p.grid.tws.len(), 10);
        assert_eq!(p.grid.twa.len(), 19);
        assert_eq!(p.blend.min_samples, 5);
        assert_eq!(p.blend.n_full, 30);
        assert!(p.blend.use_corrected);
        p.validate().unwrap();
    }

    #[test]
    fn ids_are_allocated_upwards_and_never_reused() {
        let mut p = Project::new("P", Boat::default(), 0);
        let a = p.allocate_source_id();
        let b = p.allocate_track_id();
        assert!(b.raw() > a.raw());
        assert_eq!(p.next_id, 3);
    }

    #[test]
    fn new_sources_take_the_next_unused_palette_colour() {
        let mut p = Project::new("P", Boat::default(), 0);
        let first = polar_source(&mut p);
        assert_eq!(first.colour.as_str(), PALETTE[0]);
        p.sources.push(first);
        let second = polar_source(&mut p);
        assert_eq!(second.colour.as_str(), PALETTE[1]);
    }

    #[test]
    fn duplicate_or_unallocated_ids_are_invalid() {
        let mut p = Project::new("P", Boat::default(), 0);
        let s = polar_source(&mut p);
        p.sources.push(s.clone());
        p.validate().unwrap();
        p.sources.push(s);
        assert!(p.validate().is_err());

        let mut p = Project::new("P", Boat::default(), 0);
        let mut s = polar_source(&mut p);
        s.id = SourceId(99);
        p.sources.push(s);
        assert!(p.validate().is_err());
    }
}
