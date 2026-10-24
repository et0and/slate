//! Bounded project snapshots for undo and redo.

use std::collections::HashSet;

use slate_core::model::{PhotoId, Project};

const LIMIT: usize = 40;

#[derive(Clone)]
pub(crate) struct Snapshot {
    pub project: Project,
    pub selected: HashSet<PhotoId>,
    pub dirty: bool,
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
}

impl History {
    pub fn checkpoint(&mut self, snapshot: Snapshot) {
        self.undo.push(snapshot);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn discard_last(&mut self) {
        self.undo.pop();
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let previous = self.undo.pop()?;
        self.redo.push(current);
        Some(previous)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Snapshot> {
        let next = self.redo.pop()?;
        self.undo.push(current);
        Some(next)
    }

    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Saved work is now the baseline. Any older or future snapshot differs
    /// from the file on disk, so restore it as dirty.
    pub fn mark_saved(&mut self) {
        for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            snapshot.dirty = true;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(name: &str) -> Snapshot {
        Snapshot {
            project: Project::new(name),
            selected: HashSet::new(),
            dirty: false,
        }
    }

    #[test]
    fn undo_and_redo_walk_the_stack() {
        let mut history = History::default();
        history.checkpoint(snapshot("one"));
        let one = history.undo(snapshot("two")).unwrap();
        assert_eq!(one.project.name, "one");
        assert!(history.can_redo());
        let two = history.redo(snapshot("one")).unwrap();
        assert_eq!(two.project.name, "two");
    }

    #[test]
    fn checkpoint_clears_redo() {
        let mut history = History::default();
        history.checkpoint(snapshot("one"));
        let _ = history.undo(snapshot("two"));
        history.checkpoint(snapshot("three"));
        assert!(!history.can_redo());
    }

    #[test]
    fn saving_marks_snapshots_dirty() {
        let mut history = History::default();
        history.checkpoint(snapshot("one"));
        history.mark_saved();
        let restored = history.undo(snapshot("two")).unwrap();
        assert!(restored.dirty);
    }
}
