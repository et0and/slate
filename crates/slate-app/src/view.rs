//! Cached projections of the project for the panels.

use std::collections::BTreeMap;
use std::sync::Arc;

use slate_core::model::{Filter, PhotoId, Project};

use crate::selection::Selection;

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
    search_names_ready: bool,
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
        selected: &Selection,
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

    /// Add one freshly imported photo without rescanning the whole project.
    ///
    /// Imports append to the project, so visible and scope keep their order.
    /// If the search index is not ready, fall back to a full rebuild later.
    pub(crate) fn append_imported(
        &mut self,
        photo_index: usize,
        photo: &slate_core::model::Photo,
        filter: &Filter,
        search: &str,
        in_scope: bool,
        selected: bool,
    ) {
        if self.project_dirty || self.filter_dirty || self.selection_dirty {
            return;
        }
        let needle = search.trim().to_lowercase();
        if !needle.is_empty() && !self.search_names_ready {
            self.filter_dirty = true;
            return;
        }

        if self.search_names_ready {
            self.search_names.push(photo.file_name().to_lowercase());
        }

        if photo.tags.is_empty() {
            self.untagged += 1;
        } else {
            let tags = Arc::make_mut(&mut self.tags);
            for tag in &photo.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }

        let visible = filter.matches(photo)
            && (needle.is_empty()
                || self
                    .search_names
                    .last()
                    .is_some_and(|name| name.contains(&needle)));
        if visible {
            Arc::make_mut(&mut self.visible).push(photo_index);
        }
        if in_scope && (selected || visible) {
            Arc::make_mut(&mut self.scope).push(photo.id());
        }
    }

    fn rebuild_project(&mut self, project: &Project) {
        let mut tags = BTreeMap::new();
        let mut untagged = 0;
        for photo in &project.photos {
            if photo.tags.is_empty() {
                untagged += 1;
            }
            for tag in &photo.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        self.tags = Arc::new(tags);
        self.untagged = untagged;
        self.search_names.clear();
        self.search_names_ready = false;
    }

    fn rebuild_visible(&mut self, project: &Project, filter: &Filter, search: &str) {
        let needle = search.trim().to_lowercase();
        if !needle.is_empty() && !self.search_names_ready {
            self.search_names = project
                .photos
                .iter()
                .map(|photo| photo.file_name().to_lowercase())
                .collect();
            self.search_names_ready = true;
        }
        let mut visible = Vec::with_capacity(project.photos.len());
        for (index, photo) in project.photos.iter().enumerate() {
            if !filter.matches(photo) {
                continue;
            }
            if needle.is_empty()
                || self
                    .search_names
                    .get(index)
                    .is_some_and(|name| name.contains(&needle))
            {
                visible.push(index);
            }
        }
        self.visible = Arc::new(visible);
    }

    fn rebuild_scope(&mut self, project: &Project, selected: &Selection) {
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
                .filter_map(|id| project.photos.position(id).map(|position| (position, id)))
                .collect();
            positions.sort_unstable_by_key(|(position, _)| *position);
            self.scope = Arc::new(positions.into_iter().map(|(_, id)| id).collect());
        } else {
            self.scope = Arc::new(
                project
                    .photos
                    .iter()
                    .filter(|photo| selected.contains(photo.id()))
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
        project
            .photo_mut(PhotoId(1))
            .unwrap()
            .set_tags(vec!["street".into()]);
        project
            .photo_mut(PhotoId(2))
            .unwrap()
            .set_tags(vec!["street".into()]);
        project
    }

    fn selected(ids: &[u64]) -> Selection {
        ids.iter().map(|id| PhotoId(*id)).collect()
    }

    #[test]
    fn caches_visible_tags_and_scope() {
        let project = project();
        let mut view = ViewState::new();
        let none = Selection::new();
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
        let none = Selection::new();
        view.ensure(&project, &Filter::All, "B.PNG", &none);
        assert_eq!(view.visible().as_slice(), &[1]);
    }

    #[test]
    fn imported_photos_append_to_visible_scope_and_tags() {
        let mut project = project();
        let mut view = ViewState::new();
        let none = Selection::new();
        view.ensure(&project, &Filter::All, "", &none);

        project.add_photo("/tmp/d.jpg".into(), 100, 100, 1);
        project
            .photo_mut(PhotoId(4))
            .unwrap()
            .set_tags(vec!["new".into()]);
        let photo = &project.photos[3];
        view.append_imported(3, photo, &Filter::All, "", true, false);

        assert_eq!(view.visible().as_slice(), &[0, 1, 2, 3]);
        assert_eq!(
            view.scope().as_slice(),
            &[PhotoId(1), PhotoId(2), PhotoId(3), PhotoId(4)]
        );
        assert_eq!(view.tags().get("new"), Some(&1));
    }

    #[test]
    fn project_invalidation_rebuilds_tags() {
        let mut project = project();
        let mut view = ViewState::new();
        let none = Selection::new();
        view.ensure(&project, &Filter::All, "", &none);
        project
            .photo_mut(PhotoId(3))
            .unwrap()
            .set_tags(vec!["new".into()]);
        view.invalidate_project();
        view.ensure(&project, &Filter::All, "", &none);
        assert_eq!(view.tags().get("new"), Some(&1));
        assert_eq!(view.untagged(), 0);
        assert_eq!(view.visible().as_slice(), &[0, 1, 2]);
    }
}
