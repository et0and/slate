//! Bounded project snapshots and photo-level edits for undo and redo.
//!
//! Whole-project snapshots remain cheap for sheet and saved-sheet changes
//! because `PhotoSet` is copy-on-write. Photo edits use field deltas so a
//! single rotation or capture-date change does not clone a 500k-photo library.

use std::collections::HashSet;
use std::sync::Arc;

use slate_core::model::{PhotoId, Project, QuarterTurns};

const LIMIT: usize = 40;

#[derive(Clone)]
pub(crate) struct Snapshot {
    pub project: Project,
    pub selected: Arc<HashSet<PhotoId>>,
    pub dirty: bool,
}

#[derive(Clone)]
pub(crate) enum PhotoFieldChange {
    Tags {
        id: PhotoId,
        before: Vec<String>,
        after: Vec<String>,
    },
    Rotation {
        id: PhotoId,
        before: QuarterTurns,
        after: QuarterTurns,
    },
    CapturedAt {
        id: PhotoId,
        before: Option<String>,
        after: Option<String>,
    },
}

#[derive(Clone)]
pub(crate) struct PhotoEdit {
    fields: Arc<Vec<PhotoFieldChange>>,
    before_selected: Arc<HashSet<PhotoId>>,
    after_selected: Arc<HashSet<PhotoId>>,
    before_dirty: bool,
    after_dirty: bool,
}

impl PhotoEdit {
    pub fn new(
        fields: Vec<PhotoFieldChange>,
        before_selected: Arc<HashSet<PhotoId>>,
        after_selected: Arc<HashSet<PhotoId>>,
        before_dirty: bool,
        after_dirty: bool,
    ) -> Self {
        Self {
            fields: Arc::new(fields),
            before_selected,
            after_selected,
            before_dirty,
            after_dirty,
        }
    }

    pub fn fields(&self) -> &[PhotoFieldChange] {
        &self.fields
    }

    pub fn selected(&self, forward: bool) -> Arc<HashSet<PhotoId>> {
        if forward {
            Arc::clone(&self.after_selected)
        } else {
            Arc::clone(&self.before_selected)
        }
    }

    pub fn dirty(&self, forward: bool) -> bool {
        if forward {
            self.after_dirty
        } else {
            self.before_dirty
        }
    }
}

pub(crate) enum Step {
    Snapshot(Snapshot),
    PhotoEdit(PhotoEdit),
}

#[derive(Default)]
pub(crate) struct History {
    undo: Vec<Step>,
    redo: Vec<Step>,
}

impl History {
    pub fn checkpoint(&mut self, snapshot: Snapshot) {
        self.push(Step::Snapshot(snapshot));
    }

    pub fn push_photo_edit(&mut self, edit: PhotoEdit) {
        if !edit.fields().is_empty() {
            self.push(Step::PhotoEdit(edit));
        }
    }

    fn push(&mut self, step: Step) {
        self.undo.push(step);
        if self.undo.len() > LIMIT {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    pub fn discard_last(&mut self) {
        self.undo.pop();
    }

    pub fn undo(&mut self, current: Snapshot) -> Option<Step> {
        let step = self.undo.pop()?;
        self.redo.push(match &step {
            Step::Snapshot(_) => Step::Snapshot(current),
            Step::PhotoEdit(edit) => Step::PhotoEdit(edit.clone()),
        });
        Some(step)
    }

    pub fn redo(&mut self, current: Snapshot) -> Option<Step> {
        let step = self.redo.pop()?;
        self.undo.push(match &step {
            Step::Snapshot(_) => Step::Snapshot(current),
            Step::PhotoEdit(edit) => Step::PhotoEdit(edit.clone()),
        });
        Some(step)
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

    /// Saved work is now the baseline. Any older or future state differs from
    /// the file on disk, so restore it as dirty.
    pub fn mark_saved(&mut self) {
        for step in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            match step {
                Step::Snapshot(snapshot) => snapshot.dirty = true,
                Step::PhotoEdit(edit) => {
                    edit.before_dirty = true;
                    edit.after_dirty = true;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(name: &str) -> Snapshot {
        Snapshot {
            project: Project::new(name),
            selected: Arc::new(HashSet::new()),
            dirty: false,
        }
    }

    #[test]
    fn undo_and_redo_walk_the_stack() {
        let mut history = History::default();
        history.checkpoint(snapshot("one"));
        let one = history.undo(snapshot("two")).unwrap();
        assert!(matches!(one, Step::Snapshot(snapshot) if snapshot.project.name == "one"));
        assert!(history.can_redo());
        let two = history.redo(snapshot("one")).unwrap();
        assert!(matches!(two, Step::Snapshot(snapshot) if snapshot.project.name == "two"));
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
    fn photo_edits_invert_without_project_snapshots() {
        let selected = Arc::new(HashSet::new());
        let mut history = History::default();
        history.push_photo_edit(PhotoEdit::new(
            vec![PhotoFieldChange::Rotation {
                id: PhotoId(1),
                before: QuarterTurns::NONE,
                after: QuarterTurns::try_from(1).unwrap(),
            }],
            Arc::clone(&selected),
            Arc::clone(&selected),
            false,
            true,
        ));
        let undone = history.undo(snapshot("current")).unwrap();
        assert!(matches!(
            undone,
            Step::PhotoEdit(edit) if !edit.dirty(false) && edit.fields().len() == 1
        ));
        let redone = history.redo(snapshot("current")).unwrap();
        assert!(matches!(
            redone,
            Step::PhotoEdit(edit) if edit.dirty(true) && edit.fields().len() == 1
        ));
    }

    #[test]
    fn saving_marks_snapshots_dirty() {
        let mut history = History::default();
        history.checkpoint(snapshot("one"));
        history.mark_saved();
        let restored = history.undo(snapshot("two")).unwrap();
        assert!(matches!(restored, Step::Snapshot(snapshot) if snapshot.dirty));
    }
}
