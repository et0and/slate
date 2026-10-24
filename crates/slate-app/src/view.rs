//! Cached projections of the project for the panels.

use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use slate_core::model::{Filter, PhotoId, Project};

#[derive(Default)]
pub(crate) struct ViewState {
    project_dirty: bool,
    filter_dirty: bool,
    selection_dirty: bool,
    visible: Arc<Vec<usize>>,
    scope: Arc<Vec<PhotoId>>,
    tags: Arc<BTreeMap<String, usize>>,
    untagged: usize,
    search_names: Vec<String>,
}

impl ViewState {
    pub(crate) fn new() -> Self {
        Self {
            project_dirty: true,
            filter_dirty: true,
            selection_dirty: true,
            ..Self::default()
        }
    }

    pub(crate) fn invalidate_project(&mut self) {
        self.project_dirty = true;
    }

    pub(crate) fn invalidate_filter(&mut self) {
        self.filter_dirty = true;
    }

    pub(crate) fn invalidate_selection(&mut self) {
        self.selection_dirty = true;
    }

    pub(crate) fn ensure(
        &mut self,
        project: &Project,
        filter: &Filter,
        search: &str,
        selected: &HashSet<PhotoId>,
    ) {
        if self.project_dirty {
            self.rebuild_project(project);
            self.project_dirty = false;
            self.filter_dirty = true;
            self.selection_dirty = true;
        }
        if self.filter_dirty {
            self.rebuild_visible(project, filter, search);
            self.filter_dirty = false;
            if selected.is_empty() {
                self.selection_dirty = true;
            }
        }
        if self.selection_dirty {
            self.rebuild_scope(project, selected);
            self.selection_dirty = false;
        }
    }

    pub(crate) fn visible(&self) -> Arc<Vec<usize>> {
        Arc::clone(&self.visible)
    }

    pub(crate) fn scope(&self) -> Arc<Vec<PhotoId>> {
        Arc::clone(&self.scope)
    }

    pub(crate) fn tags(&self) -> Arc<BTreeMap<String, usize>> {
        Arc::clone(&self.tags)
    }

    pub(crate) fn untagged(&self) -> usize {
        self.untagged
    }

    fn rebuild_project(&mut self, project: &Project) {
        let mut tags = BTreeMap::new();
        let mut untagged = 0;
        let mut search_names = Vec::with_capacity(project.photos.len());
        for photo in &project.photos {
            if photo.tags.is_empty() {
                untagged += 1;
            }
            for tag in &photo.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
            search_names.push(photo.file_name().to_lowercase());
        }
        self.tags = Arc::new(tags);
        self.untagged = untagged;
        self.search_names = search_names;
    }

    fn rebuild_visible(&mut self, project: &Project, filter: &Filter, search: &str) {
        let needle = search.trim().to_lowercase();
        let mut visible = Vec::with_capacity(self.search_names.len());
        for (index, photo) in project.photos.iter().enumerate() {
            let Some(name) = self.search_names.get(index) else {
                continue;
            };
            if filter.matches(photo) && (needle.is_empty() || name.contains(&needle)) {
                visible.push(index);
            }
        }
        self.visible = Arc::new(visible);
    }

    fn rebuild_scope(&mut self, project: &Project, selected: &HashSet<PhotoId>) {
        if selected.is_empty() {
            let scope = self
                .visible
                .iter()
                .filter_map(|index| project.photos.get(*index).map(|photo| photo.id()))
                .collect();
            self.scope = Arc::new(scope);
            return;
        }
        if selected.len().saturating_mul(4) < project.photos.len() {
            let mut positions: Vec<(usize, PhotoId)> = selected
                .iter()
                .filter_map(|id| project.photos.position(*id).map(|position| (position, *id)))
                .collect();
            positions.sort_unstable_by_key(|(position, _)| *position);
            self.scope = Arc::new(positions.into_iter().map(|(_, id)| id).collect());
        } else {
            self.scope = Arc::new(
                project
                    .photos
                    .iter()
                    .filter(|photo| selected.contains(&photo.id()))
                    .map(|photo| photo.id())
                    .collect(),
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_core::model::{Filter, Project};

    fn project() -> Project {
        let mut project = Project::new("Test");
        project.add_photo("/tmp/a.jpg".into(), 100, 100, 1);
        project.add_photo("/tmp/b.png".into(), 100, 100, 1);
        project.add_photo("/tmp/c.jpg".into(), 100, 100, 1);
        project.photo_mut(PhotoId(1)).unwrap().tags = vec!["street".into()];
        project.photo_mut(PhotoId(2)).unwrap().tags = vec!["street".into()];
        project
    }

    fn selected(ids: &[u64]) -> HashSet<PhotoId> {
        ids.iter().map(|id| PhotoId(*id)).collect()
    }

    #[test]
    fn caches_visible_tags_and_scope() {
        let project = project();
        let mut view = ViewState::new();
        let none = HashSet::new();
        view.ensure(&project, &Filter::All, "", &none);
        assert_eq!(view.visible().as_slice(), &[0, 1, 2]);
        assert_eq!(view.tags().get("street"), Some(&2));
        assert_eq!(view.untagged(), 1);
        assert_eq!(
            view.scope().as_slice(),
            &[PhotoId(1), PhotoId(2), PhotoId(3)]
        );

        view.invalidate_filter();
        view.ensure(&project, &Filter::Tag("street".into()), "", &none);
        assert_eq!(view.visible().as_slice(), &[0, 1]);
        assert_eq!(view.scope().as_slice(), &[PhotoId(1), PhotoId(2)]);
    }

    #[test]
    fn selection_scope_preserves_project_order() {
        let project = project();
        let mut view = ViewState::new();
        let selected = selected(&[3, 1]);
        view.ensure(&project, &Filter::All, "", &selected);
        assert_eq!(view.scope().as_slice(), &[PhotoId(1), PhotoId(3)]);
    }

    #[test]
    fn search_uses_cached_lowercase_names() {
        let project = project();
        let mut view = ViewState::new();
        let none = HashSet::new();
        view.ensure(&project, &Filter::All, "B.PNG", &none);
        assert_eq!(view.visible().as_slice(), &[1]);
    }

    #[test]
    fn project_invalidation_rebuilds_tags() {
        let mut project = project();
        let mut view = ViewState::new();
        let none = HashSet::new();
        view.ensure(&project, &Filter::All, "", &none);
        project.photo_mut(PhotoId(3)).unwrap().tags = vec!["new".into()];
        view.invalidate_project();
        view.ensure(&project, &Filter::All, "", &none);
        assert_eq!(view.tags().get("new"), Some(&1));
        assert_eq!(view.untagged(), 0);
        assert_eq!(view.visible().as_slice(), &[0, 1, 2]);
    }
}
