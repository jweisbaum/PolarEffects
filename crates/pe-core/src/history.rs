//! Undo and redo (spec.md 4.6). Ported from VectorEffects' `ve-core::history`.
//!
//! One history per project, so undo order matches the order the user acted.
//! Selection, camera and stage changes are not recorded: they are not
//! document changes, and burying real edits under them makes undo unusable.

use crate::command::Command;
use crate::error::Result;
use crate::project::Project;

/// The number of entries kept (spec.md 4.6).
pub const DEFAULT_LIMIT: usize = 200;

/// One recorded change.
#[derive(Debug, Clone)]
pub struct Entry {
    /// The change itself.
    pub command: Command,
    /// Label for the history, an English key the UI translates.
    pub label: String,
    /// Groups consecutive commands into one entry. See [`History::push_coalesced`].
    pub coalesce_key: Option<String>,
}

/// A project's undo stack.
#[derive(Debug)]
pub struct History {
    entries: Vec<Entry>,
    /// Number of entries currently applied; everything at or past this index
    /// is available to redo.
    cursor: usize,
    limit: usize,
    /// Entries ever recorded, never reduced by the limit or by undo. The
    /// autosave's "every 50 entries" trigger counts this (spec.md 4.5): the
    /// length of `entries` stops growing at the limit.
    recorded: u64,
}

impl Default for History {
    fn default() -> Self {
        Self::new(DEFAULT_LIMIT)
    }
}

impl History {
    /// An empty history keeping at most `limit` entries.
    pub fn new(limit: usize) -> Self {
        Self {
            entries: Vec::new(),
            cursor: 0,
            limit: limit.max(1),
            recorded: 0,
        }
    }

    /// Applies a command and records it.
    ///
    /// Any redo entries are discarded: the user has taken a new branch.
    pub fn push(&mut self, project: &mut Project, command: Command) -> Result<()> {
        self.push_inner(project, command, None)
    }

    /// Applies a command, merging it into the previous entry when they share
    /// a `key` and [`Command::merge`] accepts it.
    ///
    /// The key is explicit rather than time-based. A drag holds one key for
    /// its duration and drops it on release, so a gesture becomes exactly one
    /// entry however long it took.
    pub fn push_coalesced(
        &mut self,
        project: &mut Project,
        command: Command,
        key: impl Into<String>,
    ) -> Result<()> {
        self.push_inner(project, command, Some(key.into()))
    }

    fn push_inner(
        &mut self,
        project: &mut Project,
        mut command: Command,
        key: Option<String>,
    ) -> Result<()> {
        command.apply(project)?;

        // Applying succeeded, so the new state is real. Only now is it safe
        // to discard the redo branch.
        self.entries.truncate(self.cursor);

        if let Some(ref key) = key
            && let Some(last) = self.entries.last_mut()
            && last.coalesce_key.as_ref() == Some(key)
            && last.command.merge(&command)
        {
            return Ok(());
        }

        let label = command.label();
        self.entries.push(Entry {
            command,
            label,
            coalesce_key: key,
        });
        self.recorded += 1;

        if self.entries.len() > self.limit {
            let excess = self.entries.len() - self.limit;
            self.entries.drain(..excess);
        }
        self.cursor = self.entries.len();
        Ok(())
    }

    /// Reverses the most recent change, returning its label.
    pub fn undo(&mut self, project: &mut Project) -> Result<Option<String>> {
        let Some(index) = self.cursor.checked_sub(1) else {
            return Ok(None);
        };
        let Some(entry) = self.entries.get_mut(index) else {
            return Ok(None);
        };
        entry.command.undo(project)?;
        // A gesture that was undone is over: the next edit is a new entry.
        entry.coalesce_key = None;
        self.cursor = index;
        Ok(Some(entry.label.clone()))
    }

    /// Reapplies the next undone change, returning its label.
    pub fn redo(&mut self, project: &mut Project) -> Result<Option<String>> {
        let Some(entry) = self.entries.get_mut(self.cursor) else {
            return Ok(None);
        };
        entry.command.apply(project)?;
        self.cursor += 1;
        Ok(Some(entry.label.clone()))
    }

    /// Whether anything can be undone.
    pub fn can_undo(&self) -> bool {
        self.cursor > 0
    }

    /// Whether anything can be redone.
    pub fn can_redo(&self) -> bool {
        self.cursor < self.entries.len()
    }

    /// The label of what undo would reverse.
    pub fn undo_label(&self) -> Option<&str> {
        let index = self.cursor.checked_sub(1)?;
        self.entries.get(index).map(|e| e.label.as_str())
    }

    /// The label of what redo would reapply.
    pub fn redo_label(&self) -> Option<&str> {
        self.entries.get(self.cursor).map(|e| e.label.as_str())
    }

    /// Every recorded entry, oldest first.
    pub fn entries(&self) -> &[Entry] {
        &self.entries
    }

    /// How many entries are currently applied.
    pub fn cursor(&self) -> usize {
        self.cursor
    }

    /// How many entries have ever been recorded.
    pub fn recorded(&self) -> u64 {
        self.recorded
    }

    /// Ends any coalescing group, so the next edit starts a new entry.
    /// Called when a gesture ends (pointer up, blur).
    pub fn break_coalescing(&mut self) {
        if let Some(last) = self.entries.last_mut() {
            last.coalesce_key = None;
        }
    }

    /// Discards every entry, keeping the document as it is.
    pub fn clear(&mut self) {
        self.entries.clear();
        self.cursor = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixtures;

    fn weight(project: &Project) -> f64 {
        project.sources[0].weight
    }

    fn set_weight(source: crate::id::SourceId, before: f64, after: f64) -> Command {
        Command::SetSourceWeight {
            source,
            before,
            after,
        }
    }

    fn rename(before: &str, after: &str) -> Command {
        Command::RenameProject {
            before: before.to_owned(),
            after: after.to_owned(),
        }
    }

    #[test]
    fn push_applies_and_undo_reverses() {
        let mut p = fixtures::project();
        let mut h = History::default();
        h.push(&mut p, rename("Fixture", "Fastnet")).unwrap();
        assert_eq!(p.name, "Fastnet");
        assert!(h.can_undo() && !h.can_redo());
        assert_eq!(h.undo_label(), Some("Rename project"));

        assert_eq!(h.undo(&mut p).unwrap().as_deref(), Some("Rename project"));
        assert_eq!(p.name, "Fixture");
        assert!(!h.can_undo() && h.can_redo());

        assert!(h.redo(&mut p).unwrap().is_some());
        assert_eq!(p.name, "Fastnet");
    }

    #[test]
    fn undo_and_redo_stop_at_the_ends() {
        let mut p = fixtures::project();
        let mut h = History::default();
        assert!(h.undo(&mut p).unwrap().is_none());
        assert!(h.redo(&mut p).unwrap().is_none());
    }

    #[test]
    fn a_failed_command_records_nothing() {
        let mut p = fixtures::project();
        let mut h = History::default();
        assert!(h.push(&mut p, rename("Not the name", "X")).is_err());
        assert!(!h.can_undo());
        assert_eq!(p.name, "Fixture");
    }

    #[test]
    fn a_new_edit_discards_the_redo_branch() {
        let mut p = fixtures::project();
        let mut h = History::default();
        h.push(&mut p, rename("Fixture", "A")).unwrap();
        h.undo(&mut p).unwrap();
        h.push(&mut p, rename("Fixture", "B")).unwrap();
        assert!(!h.can_redo());
        assert_eq!(h.entries().len(), 1);
        assert_eq!(p.name, "B");
    }

    /// A slider drag produces many events but must leave one entry, keeping
    /// the original `before` so one undo returns to where the drag started.
    #[test]
    fn a_coalesced_drag_is_one_entry() {
        let mut p = fixtures::project();
        let s = p.sources[0].id;
        let mut h = History::default();
        let start = weight(&p);
        let mut last = start;
        for v in [1.1, 1.2, 1.3, 1.4] {
            h.push_coalesced(&mut p, set_weight(s, last, v), "weight-drag")
                .unwrap();
            last = v;
        }
        assert_eq!(h.entries().len(), 1);
        assert_eq!(weight(&p), 1.4);
        h.undo(&mut p).unwrap();
        assert_eq!(weight(&p), start, "one undo must span the whole drag");
        h.redo(&mut p).unwrap();
        assert_eq!(weight(&p), 1.4);
    }

    #[test]
    fn ending_a_gesture_starts_a_new_entry() {
        let mut p = fixtures::project();
        let s = p.sources[0].id;
        let mut h = History::default();
        h.push_coalesced(&mut p, set_weight(s, 1.0, 1.5), "drag")
            .unwrap();
        h.break_coalescing();
        h.push_coalesced(&mut p, set_weight(s, 1.5, 1.7), "drag")
            .unwrap();
        assert_eq!(h.entries().len(), 2);
        h.undo(&mut p).unwrap();
        assert_eq!(weight(&p), 1.5);
    }

    #[test]
    fn different_keys_do_not_merge() {
        let mut p = fixtures::project();
        let s = p.sources[0].id;
        let mut h = History::default();
        h.push_coalesced(&mut p, set_weight(s, 1.0, 1.1), "a")
            .unwrap();
        h.push_coalesced(&mut p, set_weight(s, 1.1, 1.2), "b")
            .unwrap();
        assert_eq!(h.entries().len(), 2);
    }

    /// An undone drag followed by another drag with the same key must not
    /// merge into the entry that is now on the redo side.
    #[test]
    fn a_drag_after_an_undo_is_a_new_entry() {
        let mut p = fixtures::project();
        let s = p.sources[0].id;
        let mut h = History::default();
        h.push_coalesced(&mut p, set_weight(s, 1.0, 1.5), "drag")
            .unwrap();
        h.undo(&mut p).unwrap();
        h.push_coalesced(&mut p, set_weight(s, 1.0, 0.5), "drag")
            .unwrap();
        assert_eq!(h.entries().len(), 1);
        h.undo(&mut p).unwrap();
        assert_eq!(weight(&p), 1.0);
    }

    #[test]
    fn the_oldest_entries_are_dropped_at_the_limit_and_still_counted() {
        let mut p = fixtures::project();
        let mut h = History::new(3);
        let mut name = "Fixture".to_owned();
        for i in 0..6 {
            let next = format!("N{i}");
            h.push(&mut p, rename(&name, &next)).unwrap();
            name = next;
        }
        assert_eq!(h.entries().len(), 3);
        assert_eq!(h.cursor(), 3);
        assert_eq!(h.recorded(), 6);
        assert_eq!(History::default().limit, 200);
    }
}
