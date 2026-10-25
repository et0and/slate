//! Compact photo selection.
//!
//! A 500k-photo "select all" must not cost a 500k-entry hash set. Roaring
//! bitmaps stay small for contiguous and sparse selections, and cloning one
//! for an undo snapshot is cheap.

use roaring::RoaringTreemap;
use slate_core::model::PhotoId;

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Selection {
    ids: RoaringTreemap,
}

impl Selection {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn insert(&mut self, id: PhotoId) -> bool {
        self.ids.insert(id.0)
    }

    pub fn remove(&mut self, id: PhotoId) -> bool {
        self.ids.remove(id.0)
    }

    pub fn contains(&self, id: PhotoId) -> bool {
        self.ids.contains(id.0)
    }

    pub fn len(&self) -> usize {
        self.ids.len() as usize
    }

    pub fn is_empty(&self) -> bool {
        self.ids.is_empty()
    }

    pub fn iter(&self) -> impl Iterator<Item = PhotoId> + '_ {
        self.ids.iter().map(PhotoId)
    }
}

impl FromIterator<PhotoId> for Selection {
    fn from_iter<I: IntoIterator<Item = PhotoId>>(iter: I) -> Self {
        let mut selection = Self::new();
        for id in iter {
            selection.insert(id);
        }
        selection
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn insert_remove_and_iterate_in_order() {
        let mut selection = Selection::new();
        assert!(selection.insert(PhotoId(10)));
        assert!(selection.insert(PhotoId(2)));
        assert!(!selection.insert(PhotoId(10)));
        assert_eq!(selection.len(), 2);
        assert_eq!(
            selection.iter().collect::<Vec<_>>(),
            vec![PhotoId(2), PhotoId(10)]
        );
        assert!(selection.remove(PhotoId(2)));
        assert!(!selection.remove(PhotoId(2)));
        assert!(!selection.contains(PhotoId(2)));
    }

    #[test]
    fn contiguous_selection_stays_compact() {
        let selection: Selection = (0..500_000).map(PhotoId).collect();
        assert_eq!(selection.len(), 500_000);
        assert!(selection.contains(PhotoId(499_999)));
        assert!(!selection.contains(PhotoId(500_000)));
    }
}
