//! Cached projections of the project for the panels.

use std::cmp::Ordering;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use slate_core::model::{ColorLabel, Filter, PhotoId, Project, StarRating};

use crate::selection::Selection;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortKey {
    Name,
    CapturedAt,
    Size,
    Type,
    Rating,
    Label,
}

impl SortKey {
    pub(crate) const ALL: [Self; 6] = [
        Self::Name,
        Self::CapturedAt,
        Self::Size,
        Self::Type,
        Self::Rating,
        Self::Label,
    ];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Name => "Name",
            Self::CapturedAt => "Date",
            Self::Size => "Size",
            Self::Type => "Type",
            Self::Rating => "Rating",
            Self::Label => "Label",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SortDirection {
    Ascending,
    Descending,
}

impl SortDirection {
    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::Ascending => "Ascending",
            Self::Descending => "Descending",
        }
    }

    pub(crate) fn toggled(self) -> Self {
        match self {
            Self::Ascending => Self::Descending,
            Self::Descending => Self::Ascending,
        }
    }
}

#[derive(Default)]
pub(crate) struct ViewState {
    project_dirty: bool,
    filter_dirty: bool,
    selection_dirty: bool,
    visible: Arc<Vec<usize>>,
    scope: Arc<Vec<PhotoId>>,
    tags: Arc<BTreeMap<String, usize>>,
    untagged: usize,
    rated: usize,
    unrated: usize,
    label_counts: [usize; ColorLabel::ALL.len()],
    scope_rating: Option<StarRating>,
    scope_label: Option<ColorLabel>,
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
        sort: SortKey,
        direction: SortDirection,
    ) {
        if self.project_dirty {
            self.rebuild_project(project);
            self.project_dirty = false;
            self.filter_dirty = true;
            self.selection_dirty = true;
        }
        if self.filter_dirty {
            self.rebuild_visible(project, filter, search, sort, direction);
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

    pub(crate) fn rated(&self) -> usize {
        self.rated
    }

    pub(crate) fn unrated(&self) -> usize {
        self.unrated
    }

    pub(crate) fn label_counts(&self) -> [usize; ColorLabel::ALL.len()] {
        self.label_counts
    }

    pub(crate) fn scope_rating(&self) -> Option<StarRating> {
        self.scope_rating
    }

    pub(crate) fn scope_label(&self) -> Option<ColorLabel> {
        self.scope_label
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
        if photo.rating().is_rated() {
            self.rated += 1;
        } else {
            self.unrated += 1;
        }
        self.label_counts[photo.label() as usize] += 1;

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
        let mut rated = 0;
        let mut unrated = 0;
        let mut label_counts = [0; ColorLabel::ALL.len()];
        for photo in &project.photos {
            if photo.tags.is_empty() {
                untagged += 1;
            }
            for tag in &photo.tags {
                *tags.entry(tag.clone()).or_insert(0) += 1;
            }
            if photo.rating().is_rated() {
                rated += 1;
            } else {
                unrated += 1;
            }
            label_counts[photo.label() as usize] += 1;
        }
        self.tags = Arc::new(tags);
        self.untagged = untagged;
        self.rated = rated;
        self.unrated = unrated;
        self.label_counts = label_counts;
        self.search_names.clear();
        self.search_names_ready = false;
    }

    fn rebuild_visible(
        &mut self,
        project: &Project,
        filter: &Filter,
        search: &str,
        sort: SortKey,
        direction: SortDirection,
    ) {
        let needle = search.trim().to_lowercase();
        let needs_names = !needle.is_empty() || sort == SortKey::Name;
        if needs_names && !self.search_names_ready {
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
        visible.sort_by(|left, right| {
            compare_photos(project, &self.search_names, *left, *right, sort, direction)
        });
        self.visible = Arc::new(visible);
    }

    fn rebuild_scope(&mut self, project: &Project, selected: &Selection) {
        let mut scope = Vec::new();
        if selected.is_empty() {
            scope.extend(
                self.visible
                    .iter()
                    .filter_map(|index| project.photos.get(*index).map(|photo| photo.id())),
            );
        } else {
            // Keep the visible sort order for selected photos. Append selected
            // photos hidden by the current filter in project order.
            let mut remaining: HashSet<PhotoId> = selected.iter().collect();
            for index in self.visible.iter() {
                let Some(photo) = project.photos.get(*index) else {
                    continue;
                };
                if remaining.remove(&photo.id()) {
                    scope.push(photo.id());
                }
            }
            if !remaining.is_empty() {
                for photo in project.photos.iter() {
                    if remaining.remove(&photo.id()) {
                        scope.push(photo.id());
                    }
                }
            }
        }
        let (rating, label) = summarize_scope(project, &scope);
        self.scope_rating = rating;
        self.scope_label = label;
        self.scope = Arc::new(scope);
    }
}

fn summarize_scope(
    project: &Project,
    scope: &[PhotoId],
) -> (Option<StarRating>, Option<ColorLabel>) {
    let mut rating: Option<StarRating> = None;
    let mut label: Option<ColorLabel> = None;
    let mut rating_mixed = false;
    let mut label_mixed = false;
    for (index, id) in scope.iter().enumerate() {
        let Some(photo) = project.photo(*id) else {
            continue;
        };
        if index == 0 {
            rating = Some(photo.rating());
            label = Some(photo.label());
        } else {
            if !rating_mixed && rating != Some(photo.rating()) {
                rating = None;
                rating_mixed = true;
            }
            if !label_mixed && label != Some(photo.label()) {
                label = None;
                label_mixed = true;
            }
        }
    }
    (rating, label)
}

fn compare_photos(
    project: &Project,
    names: &[String],
    left: usize,
    right: usize,
    sort: SortKey,
    direction: SortDirection,
) -> Ordering {
    let left_photo = project.photos.get(left);
    let right_photo = project.photos.get(right);
    let (Some(left_photo), Some(right_photo)) = (left_photo, right_photo) else {
        return left.cmp(&right);
    };
    let order = match sort {
        SortKey::Name => names
            .get(left)
            .map(String::as_str)
            .unwrap_or_default()
            .cmp(names.get(right).map(String::as_str).unwrap_or_default()),
        SortKey::CapturedAt => compare_optional(
            left_photo.captured_at.as_deref(),
            right_photo.captured_at.as_deref(),
            direction,
        ),
        SortKey::Size => left_photo.bytes.cmp(&right_photo.bytes),
        SortKey::Type => left_photo.extension().cmp(&right_photo.extension()),
        SortKey::Rating => left_photo.rating().cmp(&right_photo.rating()),
        SortKey::Label => left_photo.label().cmp(&right_photo.label()),
    };
    let order = if sort == SortKey::CapturedAt {
        order
    } else if direction == SortDirection::Descending {
        order.reverse()
    } else {
        order
    };
    order.then_with(|| left.cmp(&right))
}

fn compare_optional(left: Option<&str>, right: Option<&str>, direction: SortDirection) -> Ordering {
    match (left, right) {
        (Some(left), Some(right)) => {
            let order = left.cmp(right);
            if direction == SortDirection::Descending {
                order.reverse()
            } else {
                order
            }
        }
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => Ordering::Equal,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use slate_core::model::{ColorLabel, Filter, Project, StarRating};

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
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.visible().as_slice(), &[0, 1, 2]);
        assert_eq!(view.tags().get("street"), Some(&2));
        assert_eq!(view.untagged(), 1);
        assert_eq!(
            view.scope().as_slice(),
            &[PhotoId(1), PhotoId(2), PhotoId(3)]
        );

        view.invalidate_filter();
        view.ensure(
            &project,
            &Filter::Tag("street".into()),
            "",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.visible().as_slice(), &[0, 1]);
        assert_eq!(view.scope().as_slice(), &[PhotoId(1), PhotoId(2)]);
    }

    #[test]
    fn sorts_and_filters_by_rating_label_and_date() {
        let mut project = project();
        if let Some(photo) = project.photo_mut(PhotoId(1)) {
            photo.set_rating(StarRating::try_from(1).unwrap());
            photo.set_label(ColorLabel::Red);
            photo.captured_at = Some("2026-01-01 10:00:00".into());
        }
        if let Some(photo) = project.photo_mut(PhotoId(2)) {
            photo.set_rating(StarRating::try_from(5).unwrap());
            photo.set_label(ColorLabel::Green);
            photo.captured_at = Some("2026-02-01 10:00:00".into());
        }
        if let Some(photo) = project.photo_mut(PhotoId(3)) {
            photo.set_rating(StarRating::try_from(3).unwrap());
            photo.set_label(ColorLabel::Green);
        }
        let mut view = ViewState::new();
        let none = Selection::new();
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::Rating,
            SortDirection::Descending,
        );
        assert_eq!(view.visible().as_slice(), &[1, 2, 0]);
        assert_eq!(view.rated(), 3);
        assert_eq!(view.unrated(), 0);
        assert_eq!(view.label_counts()[ColorLabel::Green as usize], 2);

        view.invalidate_filter();
        view.ensure(
            &project,
            &Filter::Label(ColorLabel::Green),
            "",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.visible().as_slice(), &[1, 2]);

        view.invalidate_filter();
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::CapturedAt,
            SortDirection::Ascending,
        );
        assert_eq!(view.visible().as_slice(), &[0, 1, 2]);
        view.invalidate_filter();
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::CapturedAt,
            SortDirection::Descending,
        );
        assert_eq!(view.visible().as_slice(), &[1, 0, 2]);

        let selected = selected(&[2, 3]);
        view.invalidate_filter();
        view.invalidate_selection();
        view.ensure(
            &project,
            &Filter::All,
            "",
            &selected,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.scope_rating(), None);
        assert_eq!(view.scope_label(), Some(ColorLabel::Green));
    }

    #[test]
    fn selection_scope_follows_visible_order() {
        let project = project();
        let mut view = ViewState::new();
        let selected = selected(&[3, 1]);
        view.ensure(
            &project,
            &Filter::All,
            "",
            &selected,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.scope().as_slice(), &[PhotoId(1), PhotoId(3)]);
    }

    #[test]
    fn selected_scope_follows_the_visible_sort_order() {
        let project = project();
        let mut view = ViewState::new();
        let selected = selected(&[1, 3]);
        view.ensure(
            &project,
            &Filter::All,
            "",
            &selected,
            SortKey::Name,
            SortDirection::Descending,
        );
        assert_eq!(view.scope().as_slice(), &[PhotoId(3), PhotoId(1)]);
    }

    #[test]
    fn search_uses_cached_lowercase_names() {
        let project = project();
        let mut view = ViewState::new();
        let none = Selection::new();
        view.ensure(
            &project,
            &Filter::All,
            "B.PNG",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.visible().as_slice(), &[1]);
    }

    #[test]
    fn imported_photos_append_to_visible_scope_and_tags() {
        let mut project = project();
        let mut view = ViewState::new();
        let none = Selection::new();
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );

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
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );
        project
            .photo_mut(PhotoId(3))
            .unwrap()
            .set_tags(vec!["new".into()]);
        view.invalidate_project();
        view.ensure(
            &project,
            &Filter::All,
            "",
            &none,
            SortKey::Name,
            SortDirection::Ascending,
        );
        assert_eq!(view.tags().get("new"), Some(&1));
        assert_eq!(view.untagged(), 0);
        assert_eq!(view.visible().as_slice(), &[0, 1, 2]);
    }
}
