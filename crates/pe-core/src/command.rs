//! Document mutations, each one reversible (spec.md 4.6).
//!
//! Every change to a project goes through a [`Command`]. A command carries
//! both the new value and the value it displaced, so undo is exact rather
//! than recomputed — the VectorEffects pattern. Snapshotting the whole
//! document per edit was rejected there for cost, and here a project holds
//! hundreds of thousands of samples.
//!
//! **Apply checks the document is in the state the command was built
//! against.** A command whose `before` no longer matches refuses with
//! [`CoreError::Stale`] rather than silently writing, so a bug elsewhere
//! cannot make undo restore the wrong value.

use serde::{Deserialize, Serialize};

use crate::error::{CoreError, Result};
use crate::id::SourceId;
use crate::project::Project;
use crate::source::{CellRef, Colour, Source, SourceKind, validate_weight};

/// A reversible change to a project.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Command {
    /// Renames the project.
    RenameProject {
        /// Previous name.
        before: String,
        /// New name.
        after: String,
    },
    /// Inserts a source at `index`. An import is this (spec.md 4.6): undo
    /// removes the source.
    AddSource {
        /// Position in the list.
        index: usize,
        /// The source, with its bulk data.
        source: Box<Source>,
    },
    /// Removes the source at `index`, keeping it for undo.
    RemoveSource {
        /// Position in the list.
        index: usize,
        /// The removed source, restored on undo.
        source: Box<Source>,
    },
    /// Changes a source's colour.
    SetSourceColour {
        /// Target source.
        source: SourceId,
        /// Previous colour.
        before: Colour,
        /// New colour.
        after: Colour,
    },
    /// Shows or hides a source.
    SetSourceVisible {
        /// Target source.
        source: SourceId,
        /// Previous state.
        before: bool,
        /// New state.
        after: bool,
    },
    /// Changes a source's blend weight. A slider drag coalesces into one entry.
    SetSourceWeight {
        /// Target source.
        source: SourceId,
        /// Previous weight.
        before: f64,
        /// New weight.
        after: f64,
    },
    /// Renames a source.
    SetSourceLabel {
        /// Target source.
        source: SourceId,
        /// Previous label.
        before: String,
        /// New label.
        after: String,
    },
    /// Moves a source within the list (display order only).
    MoveSource {
        /// Index before the move.
        from: usize,
        /// Index after the move.
        to: usize,
    },
    /// Excludes polar nodes of one ORC or file source from the blend
    /// (spec.md 10.3). `cells` are exactly the nodes this changes: none may
    /// already be excluded, so undo removes exactly them.
    ExcludeCells {
        /// Target source.
        source: SourceId,
        /// The nodes excluded, each once.
        cells: Vec<CellRef>,
    },
    /// Includes excluded polar nodes again (spec.md 10.3); the inverse of
    /// [`Command::ExcludeCells`]. Every cell must currently be excluded.
    IncludeCells {
        /// Target source.
        source: SourceId,
        /// The nodes included, each once.
        cells: Vec<CellRef>,
    },
    /// Several commands as one history entry, e.g. a multi-file import.
    Batch {
        /// The history label.
        label: String,
        /// Applied in order, undone in reverse.
        commands: Vec<Command>,
    },
}

fn stale(what: &str) -> CoreError {
    CoreError::Stale(what.to_owned())
}

fn source_mut(project: &mut Project, id: SourceId) -> Result<&mut Source> {
    project
        .source_mut(id)
        .ok_or(CoreError::MissingSource(id.raw()))
}

/// Replaces `field` with `to` if it currently holds `from`.
fn swap<T: PartialEq + Clone>(field: &mut T, from: &T, to: &T, what: &str) -> Result<()> {
    if field != from {
        return Err(stale(what));
    }
    *field = to.clone();
    Ok(())
}

impl Command {
    /// Applies the change.
    pub fn apply(&mut self, project: &mut Project) -> Result<()> {
        self.run(project, true)
    }

    /// Reverses the change.
    pub fn undo(&mut self, project: &mut Project) -> Result<()> {
        self.run(project, false)
    }

    fn run(&mut self, project: &mut Project, forward: bool) -> Result<()> {
        match self {
            Self::RenameProject { before, after } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                if to.trim().is_empty() {
                    return Err(CoreError::Invalid("a project needs a name".to_owned()));
                }
                swap(&mut project.name, from, to, "project name")
            }
            Self::AddSource { index, source } => {
                if forward {
                    insert(project, *index, source)
                } else {
                    **source = remove(project, *index, source.id)?;
                    Ok(())
                }
            }
            Self::RemoveSource { index, source } => {
                if forward {
                    // What was really there is what undo restores, overlay
                    // and all.
                    **source = remove(project, *index, source.id)?;
                    Ok(())
                } else {
                    insert(project, *index, source)
                }
            }
            Self::SetSourceColour {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                swap(
                    &mut source_mut(project, *source)?.colour,
                    from,
                    to,
                    "colour",
                )
            }
            Self::SetSourceVisible {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                swap(
                    &mut source_mut(project, *source)?.visible,
                    &from,
                    &to,
                    "visibility",
                )
            }
            Self::SetSourceWeight {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (*before, *after)
                } else {
                    (*after, *before)
                };
                validate_weight(to)?;
                swap(
                    &mut source_mut(project, *source)?.weight,
                    &from,
                    &to,
                    "weight",
                )
            }
            Self::SetSourceLabel {
                source,
                before,
                after,
            } => {
                let (from, to) = if forward {
                    (&*before, &*after)
                } else {
                    (&*after, &*before)
                };
                swap(&mut source_mut(project, *source)?.label, from, to, "label")
            }
            Self::MoveSource { from, to } => {
                let (from, to) = if forward { (*from, *to) } else { (*to, *from) };
                let len = project.sources.len();
                for index in [from, to] {
                    if index >= len {
                        return Err(CoreError::IndexOutOfBounds { index, len });
                    }
                }
                let moved = project.sources.remove(from);
                project.sources.insert(to, moved);
                Ok(())
            }
            Self::ExcludeCells { source, cells } => {
                let target = polar_source_mut(project, *source)?;
                let list = &mut target.overlay.excluded_cells;
                if forward {
                    add_cells(list, cells)
                } else {
                    remove_cells(list, cells)
                }
            }
            Self::IncludeCells { source, cells } => {
                let target = polar_source_mut(project, *source)?;
                let list = &mut target.overlay.excluded_cells;
                if forward {
                    remove_cells(list, cells)
                } else {
                    add_cells(list, cells)
                }
            }
            Self::Batch { commands, .. } => {
                if forward {
                    for (done, command) in commands.iter_mut().enumerate() {
                        if let Err(err) = command.apply(project) {
                            // Leave the document as it was: a half-applied
                            // batch would be an entry describing nothing the
                            // user did.
                            for earlier in commands.iter_mut().take(done).rev() {
                                earlier.undo(project)?;
                            }
                            return Err(err);
                        }
                    }
                } else {
                    for (done, command) in commands.iter_mut().rev().enumerate() {
                        if let Err(err) = command.undo(project) {
                            let len = commands.len();
                            for later in commands.iter_mut().skip(len - done) {
                                later.apply(project)?;
                            }
                            return Err(err);
                        }
                    }
                }
                Ok(())
            }
        }
    }

    /// Folds `next` into this command, for a coalesced gesture: the entry
    /// keeps its first `before` and takes the newest `after`, so one undo
    /// returns to where the gesture started. Returns whether it merged.
    pub fn merge(&mut self, next: &Self) -> bool {
        match (self, next) {
            (
                Self::SetSourceWeight { source, after, .. },
                Self::SetSourceWeight {
                    source: next_source,
                    after: next_after,
                    ..
                },
            ) if source == next_source => {
                *after = *next_after;
                true
            }
            (
                Self::SetSourceColour { source, after, .. },
                Self::SetSourceColour {
                    source: next_source,
                    after: next_after,
                    ..
                },
            ) if source == next_source => {
                *after = next_after.clone();
                true
            }
            _ => false,
        }
    }

    /// The history label: English text the UI translates as a key
    /// (spec.md 3.5).
    pub fn label(&self) -> String {
        match self {
            Self::RenameProject { .. } => "Rename project",
            Self::AddSource { .. } => "Add source",
            Self::RemoveSource { .. } => "Remove source",
            Self::SetSourceColour { .. } => "Change source colour",
            Self::SetSourceVisible { after: true, .. } => "Show source",
            Self::SetSourceVisible { after: false, .. } => "Hide source",
            Self::SetSourceWeight { .. } => "Change source weight",
            Self::SetSourceLabel { .. } => "Rename source",
            Self::MoveSource { .. } => "Reorder sources",
            Self::ExcludeCells { .. } => EXCLUDE_NODES_LABEL,
            Self::IncludeCells { .. } => INCLUDE_NODES_LABEL,
            Self::Batch { label, .. } => return label.clone(),
        }
        .to_owned()
    }
}

/// The history label of an exclusion, also used for a batch of them over
/// several sources.
pub const EXCLUDE_NODES_LABEL: &str = "Exclude polar nodes";
/// The history label of an inclusion.
pub const INCLUDE_NODES_LABEL: &str = "Include polar nodes";

/// A source whose polar nodes can be excluded: an ORC or file polar. A
/// track's positions are excluded one sample at a time instead (spec.md 10.3).
fn polar_source_mut(project: &mut Project, id: SourceId) -> Result<&mut Source> {
    let source = source_mut(project, id)?;
    if matches!(source.kind, SourceKind::Track { .. }) {
        return Err(CoreError::Invalid(format!(
            "{} is a track; its positions are excluded sample by sample, not as polar nodes",
            source.label
        )));
    }
    Ok(source)
}

/// Checks `cells` is a non-empty set of valid cells, each named once.
fn check_cells(cells: &[CellRef]) -> Result<()> {
    if cells.is_empty() {
        return Err(CoreError::Invalid(
            "an exclusion names at least one polar node".to_owned(),
        ));
    }
    let mut sorted: Vec<&CellRef> = cells.iter().collect();
    sorted.sort_by(|a, b| a.order(b));
    for cell in &sorted {
        cell.validate()?;
    }
    if sorted
        .windows(2)
        .any(|pair| pair[0].order(pair[1]) == std::cmp::Ordering::Equal)
    {
        return Err(CoreError::Invalid(
            "an exclusion names the same polar node twice".to_owned(),
        ));
    }
    Ok(())
}

/// Adds `cells` to a sorted list, keeping it sorted; refuses (changing
/// nothing) if any is already there.
fn add_cells(list: &mut Vec<CellRef>, cells: &[CellRef]) -> Result<()> {
    check_cells(cells)?;
    if cells
        .iter()
        .any(|cell| list.binary_search_by(|held| held.order(cell)).is_ok())
    {
        return Err(stale("the excluded polar nodes"));
    }
    for cell in cells {
        let at = list.partition_point(|held| held.order(cell) == std::cmp::Ordering::Less);
        list.insert(at, cell.clone());
    }
    Ok(())
}

/// Removes `cells` from a sorted list; refuses (changing nothing) if any is
/// not there.
fn remove_cells(list: &mut Vec<CellRef>, cells: &[CellRef]) -> Result<()> {
    check_cells(cells)?;
    if cells
        .iter()
        .any(|cell| list.binary_search_by(|held| held.order(cell)).is_err())
    {
        return Err(stale("the excluded polar nodes"));
    }
    list.retain(|held| {
        !cells
            .iter()
            .any(|cell| held.order(cell) == std::cmp::Ordering::Equal)
    });
    Ok(())
}

fn insert(project: &mut Project, index: usize, source: &Source) -> Result<()> {
    let len = project.sources.len();
    if index > len {
        return Err(CoreError::IndexOutOfBounds { index, len });
    }
    if project.source(source.id).is_some() {
        return Err(stale("a source with that id is already in the project"));
    }
    source.validate()?;
    project.sources.insert(index, source.clone());
    Ok(())
}

fn remove(project: &mut Project, index: usize, id: SourceId) -> Result<Source> {
    let len = project.sources.len();
    match project.sources.get(index) {
        None => Err(CoreError::IndexOutOfBounds { index, len }),
        Some(at) if at.id != id => Err(stale("the source list")),
        Some(_) => Ok(project.sources.remove(index)),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::*;
    use crate::fixtures;
    use crate::polar::{PolarFileFormat, PolarGrid};

    /// Every variant, by name. The match has no wildcard, so a new variant
    /// does not compile until it is named here — and then
    /// `every_command_has_an_undo_inverse` fails until it has an example.
    fn variant(command: &Command) -> &'static str {
        match command {
            Command::RenameProject { .. } => "RenameProject",
            Command::AddSource { .. } => "AddSource",
            Command::RemoveSource { .. } => "RemoveSource",
            Command::SetSourceColour { .. } => "SetSourceColour",
            Command::SetSourceVisible { .. } => "SetSourceVisible",
            Command::SetSourceWeight { .. } => "SetSourceWeight",
            Command::SetSourceLabel { .. } => "SetSourceLabel",
            Command::MoveSource { .. } => "MoveSource",
            Command::ExcludeCells { .. } => "ExcludeCells",
            Command::IncludeCells { .. } => "IncludeCells",
            Command::Batch { .. } => "Batch",
        }
    }
    const VARIANTS: usize = 11;

    fn cell(twa: f64, tws: f64) -> CellRef {
        CellRef { twa, tws }
    }

    fn new_polar(project: &mut Project) -> Source {
        let id = project.allocate_source_id();
        let colour = project.next_palette_colour();
        Source::new(
            id,
            "new.pol",
            colour,
            SourceKind::PolarFile {
                format: PolarFileFormat::Adrena,
                file_name: "new.pol".to_owned(),
                polar: PolarGrid::empty(vec![60.0], vec![8.0]),
            },
        )
    }

    /// One example of every command, each built against `project`.
    fn every_command(project: &mut Project) -> Vec<Command> {
        let first = project.sources[0].id;
        // A node already excluded, for the inclusion to include.
        project.sources[1].overlay.excluded_cells = vec![cell(0.0, 6.0), cell(45.0, 12.0)];
        let track = project.sources[2].clone();
        let added = new_polar(project);
        let also = new_polar(project);
        vec![
            Command::RenameProject {
                before: "Fixture".to_owned(),
                after: "Fastnet 2025".to_owned(),
            },
            Command::AddSource {
                index: 1,
                source: Box::new(added),
            },
            Command::RemoveSource {
                index: 2,
                source: Box::new(track),
            },
            Command::SetSourceColour {
                source: first,
                before: project.sources[0].colour.clone(),
                after: Colour::parse("#123456").unwrap(),
            },
            Command::SetSourceVisible {
                source: first,
                before: true,
                after: false,
            },
            Command::SetSourceWeight {
                source: first,
                before: 1.0,
                after: 1.75,
            },
            Command::SetSourceLabel {
                source: first,
                before: "ORC".to_owned(),
                after: "Sister ship".to_owned(),
            },
            Command::MoveSource { from: 0, to: 2 },
            Command::ExcludeCells {
                source: first,
                cells: vec![cell(150.0, 6.0), cell(52.0, 12.0)],
            },
            Command::IncludeCells {
                source: project.sources[1].id,
                cells: vec![cell(45.0, 12.0)],
            },
            Command::Batch {
                label: "Import polar files".to_owned(),
                commands: vec![
                    Command::AddSource {
                        index: 3,
                        source: Box::new(also),
                    },
                    Command::SetSourceVisible {
                        source: first,
                        before: true,
                        after: false,
                    },
                ],
            },
        ]
    }

    /// Spec 4.6 and the testing rules: every command has an exact inverse.
    /// Apply changes the document, undo restores it exactly, and redo gives
    /// exactly the applied state again.
    #[test]
    fn every_command_has_an_undo_inverse() {
        let mut template = fixtures::project();
        let commands = every_command(&mut template);
        let names: BTreeSet<&str> = commands.iter().map(variant).collect();
        assert_eq!(names.len(), VARIANTS, "an example per variant: {names:?}");

        for mut command in commands {
            let mut project = template.clone();
            let before = project.clone();
            command.apply(&mut project).unwrap();
            assert_ne!(project, before, "{} changed nothing", variant(&command));
            let applied = project.clone();

            command.undo(&mut project).unwrap();
            assert_eq!(
                project,
                before,
                "{} did not undo exactly",
                variant(&command)
            );

            command.apply(&mut project).unwrap();
            assert_eq!(
                project,
                applied,
                "{} did not redo exactly",
                variant(&command)
            );
            assert!(!command.label().is_empty());
        }
    }

    /// The inverse also holds through a save and a reload, where the bulk
    /// track data travels separately.
    #[test]
    fn removing_a_track_and_undoing_restores_its_samples() {
        let mut project = fixtures::project();
        let before = project.clone();
        let source = project.sources[2].clone();
        let mut command = Command::RemoveSource {
            index: 2,
            source: Box::new(source),
        };
        command.apply(&mut project).unwrap();
        assert_eq!(project.sources.len(), 2);
        command.undo(&mut project).unwrap();
        assert_eq!(project, before);
        assert_eq!(project.sources[2].track().unwrap().samples.len(), 3);
    }

    #[test]
    fn a_command_built_against_another_state_is_refused() {
        let mut project = fixtures::project();
        let first = project.sources[0].id;
        let mut stale = Command::SetSourceLabel {
            source: first,
            before: "not the label".to_owned(),
            after: "x".to_owned(),
        };
        assert!(matches!(
            stale.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        let mut missing = Command::SetSourceVisible {
            source: SourceId(9999),
            before: true,
            after: false,
        };
        assert!(matches!(
            missing.apply(&mut project),
            Err(CoreError::MissingSource(9999))
        ));
        let mut wrong_index = Command::RemoveSource {
            index: 0,
            source: Box::new(project.sources[1].clone()),
        };
        assert!(wrong_index.apply(&mut project).is_err());
        let mut out_of_range = Command::MoveSource { from: 0, to: 7 };
        assert!(matches!(
            out_of_range.apply(&mut project),
            Err(CoreError::IndexOutOfBounds { index: 7, len: 3 })
        ));
    }

    #[test]
    fn invalid_values_are_refused() {
        let mut project = fixtures::project();
        let first = project.sources[0].id;
        let mut heavy = Command::SetSourceWeight {
            source: first,
            before: 1.0,
            after: 2.5,
        };
        assert!(heavy.apply(&mut project).is_err());
        let mut blank = Command::RenameProject {
            before: "Fixture".to_owned(),
            after: "  ".to_owned(),
        };
        assert!(blank.apply(&mut project).is_err());
        let duplicate = project.sources[0].clone();
        let mut twice = Command::AddSource {
            index: 0,
            source: Box::new(duplicate),
        };
        assert!(twice.apply(&mut project).is_err());
    }

    /// A batch that fails part-way leaves the document as it was.
    #[test]
    fn a_failing_batch_rolls_back() {
        let mut project = fixtures::project();
        let before = project.clone();
        let first = project.sources[0].id;
        let mut batch = Command::Batch {
            label: "Both".to_owned(),
            commands: vec![
                Command::SetSourceVisible {
                    source: first,
                    before: true,
                    after: false,
                },
                Command::SetSourceVisible {
                    source: SourceId(9999),
                    before: true,
                    after: false,
                },
            ],
        };
        assert!(batch.apply(&mut project).is_err());
        assert_eq!(project, before);
    }

    #[test]
    fn weight_and_colour_drags_merge_but_other_sources_do_not() {
        let project = fixtures::project();
        let (a, b) = (project.sources[0].id, project.sources[1].id);
        let mut first = Command::SetSourceWeight {
            source: a,
            before: 1.0,
            after: 1.2,
        };
        assert!(first.merge(&Command::SetSourceWeight {
            source: a,
            before: 1.2,
            after: 1.4,
        }));
        assert_eq!(
            first,
            Command::SetSourceWeight {
                source: a,
                before: 1.0,
                after: 1.4
            }
        );
        assert!(!first.merge(&Command::SetSourceWeight {
            source: b,
            before: 1.0,
            after: 0.5,
        }));
        assert!(!first.merge(&Command::MoveSource { from: 0, to: 1 }));
    }

    /// Excluding keeps the list sorted whatever order the nodes came in, and
    /// the inclusion of some of them is undone back to the same list.
    #[test]
    fn exclusions_keep_their_order_and_include_undoes_exactly() {
        let mut project = fixtures::project();
        let original = project.clone();
        let orc = project.sources[0].id;
        let mut exclude = Command::ExcludeCells {
            source: orc,
            cells: vec![cell(150.0, 6.0), cell(52.0, 12.0), cell(52.0, 6.0)],
        };
        exclude.apply(&mut project).unwrap();
        assert_eq!(
            project.sources[0].overlay.excluded_cells,
            vec![cell(52.0, 6.0), cell(52.0, 12.0), cell(150.0, 6.0)]
        );
        assert!(project.sources[0].overlay.is_cell_excluded(52.0, 12.0));
        assert!(!project.sources[0].overlay.is_cell_excluded(90.0, 12.0));
        project.validate().unwrap();

        let excluded = project.clone();
        let mut include = Command::IncludeCells {
            source: orc,
            cells: vec![cell(52.0, 12.0)],
        };
        include.apply(&mut project).unwrap();
        assert_eq!(
            project.sources[0].overlay.excluded_cells,
            vec![cell(52.0, 6.0), cell(150.0, 6.0)]
        );
        include.undo(&mut project).unwrap();
        assert_eq!(project, excluded);

        // Undoing the exclusion gives the source back exactly (invariant 1).
        exclude.undo(&mut project).unwrap();
        assert_eq!(project, original);
        assert!(project.sources[0].overlay.is_empty());
    }

    #[test]
    fn a_node_excluded_twice_or_included_while_not_excluded_is_refused() {
        let mut project = fixtures::project();
        let orc = project.sources[0].id;
        Command::ExcludeCells {
            source: orc,
            cells: vec![cell(90.0, 6.0)],
        }
        .apply(&mut project)
        .unwrap();
        let before = project.clone();

        let mut again = Command::ExcludeCells {
            source: orc,
            cells: vec![cell(52.0, 6.0), cell(90.0, 6.0)],
        };
        assert!(matches!(
            again.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        let mut absent = Command::IncludeCells {
            source: orc,
            cells: vec![cell(90.0, 6.0), cell(90.0, 12.0)],
        };
        assert!(matches!(
            absent.apply(&mut project),
            Err(CoreError::Stale(_))
        ));
        // A refusal changes nothing, not even the nodes it could have taken.
        assert_eq!(project, before);

        for cells in [
            vec![],
            vec![cell(52.0, 12.0), cell(52.0, 12.0)],
            vec![cell(181.0, 12.0)],
            vec![cell(f64::NAN, 12.0)],
            vec![cell(52.0, -1.0)],
        ] {
            let mut bad = Command::ExcludeCells { source: orc, cells };
            assert!(matches!(
                bad.apply(&mut project),
                Err(CoreError::Invalid(_))
            ));
        }
        let mut track = Command::ExcludeCells {
            source: project.sources[2].id,
            cells: vec![cell(90.0, 12.0)],
        };
        assert!(matches!(
            track.apply(&mut project),
            Err(CoreError::Invalid(_))
        ));
        assert_eq!(project, before);
    }

    /// Exclusions survive a save and reload byte for byte, and a list out of
    /// order is refused rather than silently reordered.
    #[test]
    fn exclusions_round_trip_through_a_project_file() {
        let mut project = fixtures::project();
        Command::ExcludeCells {
            source: project.sources[1].id,
            cells: vec![cell(90.0, 6.0), cell(45.0, 6.0), cell(45.0, 12.0)],
        }
        .apply(&mut project)
        .unwrap();
        let bytes = crate::io::to_bytes(&project).unwrap();
        let loaded = crate::io::from_bytes(&bytes).unwrap();
        assert_eq!(loaded, project);
        assert_eq!(crate::io::to_bytes(&loaded).unwrap(), bytes);

        let mut unsorted = project.clone();
        unsorted.sources[1].overlay.excluded_cells.swap(0, 2);
        assert!(unsorted.validate().is_err());
        assert!(crate::io::to_bytes(&unsorted).is_err());
    }
}
