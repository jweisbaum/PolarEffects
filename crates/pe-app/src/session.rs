//! The open project and what a session adds to it (spec.md 3.3).
//!
//! Ported from VectorEffects' `session.rs`. One project is open at a time.
//! The document lives in `pe-core`; this module owns where it came from,
//! whether it has unsaved changes, its revision and its undo stack. Dirty
//! state lives here, in Rust, never in the frontend.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use pe_core::{Command, History, Project, io};

use crate::error::{AppError, Context, Result};
use crate::settings::Settings;

static LAST_REVISION: AtomicU64 = AtomicU64::new(0);

/// A revision no project has used before, in this run or an earlier one.
///
/// Revisions let the frontend tell one state of one opening from another. Two
/// different documents must never share one, so each opening starts from the
/// clock rather than from 1. Never reaches a project file.
pub fn fresh_revision() -> u64 {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_millis() as u64);
    LAST_REVISION
        .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |last| {
            Some(last.max(now).saturating_add(1))
        })
        .map_or(now, |last| last.max(now).saturating_add(1))
}

/// A project open for editing.
#[derive(Debug)]
pub struct OpenProject {
    /// The document.
    pub project: Project,
    /// Its undo stack.
    pub history: History,
    /// Where it was loaded from or last saved to; `None` until first saved.
    pub path: Option<PathBuf>,
    /// Whether there are changes not yet written to disk.
    pub dirty: bool,
    /// Bumped on every document change; unique to this opening.
    pub revision: u64,
    /// How many times this opening has been saved. The autosave compares it
    /// before and after writing a snapshot outside the lock: a save in
    /// between makes the snapshot stale, and it is removed.
    pub saves: u64,
    /// What is derived from each source, recomputed only when that source
    /// changes; never saved (invariant 2).
    pub derived: crate::derived::Derivations,
}

impl OpenProject {
    /// Wraps a freshly created project. A new project starts dirty: it exists
    /// only in memory, so closing it really would lose it (spec.md 3.3).
    pub fn created(project: Project) -> Self {
        let revision = fresh_revision();
        Self {
            project,
            history: History::default(),
            path: None,
            dirty: true,
            revision,
            saves: 0,
            derived: crate::derived::Derivations::for_opening(revision),
        }
    }

    /// Wraps a project loaded from disk.
    pub fn loaded(project: Project, path: PathBuf) -> Self {
        let revision = fresh_revision();
        Self {
            project,
            history: History::default(),
            path: Some(path),
            dirty: false,
            revision,
            saves: 0,
            derived: crate::derived::Derivations::for_opening(revision),
        }
    }

    /// Records a document change the caller cannot place: everything
    /// derived is recomputed.
    pub fn touch(&mut self) {
        self.mark();
        self.derived.invalidate_all();
    }

    /// Records that a track's samples were written outside a command (the
    /// environment fetch): only that track's derived data is recomputed.
    pub fn touch_samples(&mut self, source: u64) {
        self.mark();
        self.derived.samples_changed(source);
    }

    fn mark(&mut self) {
        self.revision = self.revision.wrapping_add(1);
        self.dirty = true;
    }

    /// Applies a command through the history and marks the change.
    pub fn apply(&mut self, command: Command) -> Result<()> {
        let touches = crate::derived::touches(&command);
        self.history.push(&mut self.project, command)?;
        self.derived.record(&touches);
        self.derived.prune(&self.project);
        self.mark();
        Ok(())
    }

    /// Applies a command that coalesces with the previous one of the same
    /// gesture (a slider or a node drag).
    pub fn apply_coalesced(&mut self, command: Command, key: &str) -> Result<()> {
        let touches = crate::derived::touches(&command);
        self.history
            .push_coalesced(&mut self.project, command, key)?;
        self.derived.record(&touches);
        self.derived.prune(&self.project);
        self.mark();
        Ok(())
    }

    /// Reverses the most recent change; whether there was one.
    pub fn undo(&mut self) -> Result<bool> {
        let touches = self
            .history
            .cursor()
            .checked_sub(1)
            .and_then(|at| self.history.entries().get(at))
            .map(|entry| crate::derived::touches(&entry.command));
        if self.history.undo(&mut self.project)?.is_none() {
            return Ok(false);
        }
        self.derived.record(&touches.unwrap_or_default());
        self.derived.prune(&self.project);
        self.mark();
        Ok(true)
    }

    /// Reapplies the most recently undone change; whether there was one.
    pub fn redo(&mut self) -> Result<bool> {
        let touches = self
            .history
            .entries()
            .get(self.history.cursor())
            .map(|entry| crate::derived::touches(&entry.command));
        if self.history.redo(&mut self.project)?.is_none() {
            return Ok(false);
        }
        self.derived.record(&touches.unwrap_or_default());
        self.derived.prune(&self.project);
        self.mark();
        Ok(true)
    }
}

/// Everything the running application holds beyond static configuration.
#[derive(Debug, Default)]
pub struct Session {
    /// The open project, if any.
    pub open: Option<OpenProject>,
    /// Global settings, including the recent list.
    pub settings: Settings,
}

impl Session {
    /// A session with the settings read from disk.
    pub fn load(settings_file: &Path) -> Self {
        Self {
            open: None,
            settings: Settings::load(settings_file),
        }
    }

    /// The open project, or an error saying there is none.
    pub fn require_open(&mut self) -> Result<&mut OpenProject> {
        self.open.as_mut().ok_or(AppError::NoProjectOpen)
    }

    /// Refuses to drop an open project that has unsaved changes.
    ///
    /// Every path that replaces or closes the open project calls this with
    /// the user's answer. Nothing is dropped until the replacing call itself
    /// runs, so a user who says "don't save" and then cancels a file dialog
    /// still has their project.
    pub fn refuse_to_discard(&self, discard_unsaved: bool) -> Result<()> {
        match &self.open {
            Some(open) if open.dirty && !discard_unsaved => Err(AppError::UnsavedChanges {
                name: open.project.name.clone(),
            }),
            _ => Ok(()),
        }
    }

    /// Saves the open project to `path` and remembers it as recent.
    pub fn save_to(&mut self, path: PathBuf) -> Result<()> {
        let open = self.require_open()?;
        io::save(&open.project, &path).doing("save the project to", path.display())?;
        open.path = Some(path.clone());
        open.dirty = false;
        open.saves += 1;
        self.settings.remember(&path);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pe_core::Boat;

    fn project() -> Project {
        Project::new("T", Boat::default(), 0)
    }

    #[test]
    fn a_new_project_starts_dirty_and_unsaved() {
        let open = OpenProject::created(project());
        assert!(open.dirty);
        assert!(open.path.is_none());
    }

    #[test]
    fn a_loaded_project_starts_clean() {
        let open = OpenProject::loaded(project(), PathBuf::from("/tmp/x.wpsproj"));
        assert!(!open.dirty);
    }

    #[test]
    fn a_change_bumps_the_revision_and_marks_it_dirty() {
        let mut open = OpenProject::loaded(project(), PathBuf::from("/tmp/x.wpsproj"));
        let before = open.revision;
        open.apply(Command::RenameProject {
            before: "T".to_owned(),
            after: "U".to_owned(),
        })
        .unwrap();
        assert!(open.revision > before);
        assert!(open.dirty);
    }

    #[test]
    fn a_refused_change_leaves_the_project_clean() {
        let mut open = OpenProject::loaded(project(), PathBuf::from("/tmp/x.wpsproj"));
        let before = open.revision;
        assert!(
            open.apply(Command::RenameProject {
                before: "not T".to_owned(),
                after: "U".to_owned(),
            })
            .is_err()
        );
        assert_eq!(open.revision, before);
        assert!(!open.dirty);
    }

    #[test]
    fn two_openings_never_share_a_revision() {
        let a = OpenProject::created(project());
        let b = OpenProject::created(project());
        assert!(a.revision < b.revision);
    }

    #[test]
    fn operations_needing_a_project_say_so() {
        let mut session = Session::default();
        assert!(matches!(
            session.require_open(),
            Err(AppError::NoProjectOpen)
        ));
    }
}
