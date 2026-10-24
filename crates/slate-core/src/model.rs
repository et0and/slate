//! Project data: photos, sheet settings and saved sheets.
//!
//! Everything in this module serializes to the on-disk `.contactsheet` file,
//! so a project can always be reopened and exported later.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

pub const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PhotoId(pub u64);

impl std::fmt::Display for PhotoId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SheetId(pub u64);

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    pub id: PhotoId,
    pub path: PathBuf,
    pub width: u32,
    pub height: u32,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    /// User rotation in clockwise quarter turns.
    #[serde(default)]
    pub rotation: u8,
}

impl Photo {
    pub fn file_name(&self) -> String {
        self.path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_else(|| self.path.to_string_lossy().into_owned())
    }

    pub fn extension(&self) -> String {
        self.path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    }

    pub fn rotation(&self) -> u8 {
        self.rotation % 4
    }

    pub fn rotate_clockwise(&mut self) {
        self.rotation = (self.rotation + 1) % 4;
    }

    pub fn oriented_size(&self) -> (u32, u32) {
        if self.rotation().is_multiple_of(2) {
            (self.width, self.height)
        } else {
            (self.height, self.width)
        }
    }

    pub fn aspect(&self) -> f32 {
        let (width, height) = self.oriented_size();
        if height == 0 {
            1.0
        } else {
            width as f32 / height as f32
        }
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|existing| existing == tag)
    }

    pub fn is_portrait(&self) -> bool {
        let (width, height) = self.oriented_size();
        height > width
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Paper {
    #[default]
    A4,
    A3,
    Letter,
    Tabloid,
}

impl Paper {
    pub const ALL: [Paper; 4] = [Paper::A4, Paper::A3, Paper::Letter, Paper::Tabloid];

    /// Portrait width and height, in millimetres.
    pub fn mm(self) -> (f32, f32) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
            Paper::Letter => (215.9, 279.4),
            Paper::Tabloid => (279.4, 431.8),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::A3 => "A3",
            Paper::Letter => "Letter",
            Paper::Tabloid => "Tabloid",
        }
    }

    pub fn next(self) -> Self {
        let index = Self::ALL
            .iter()
            .position(|paper| *paper == self)
            .unwrap_or(0);
        Self::ALL[(index + 1) % Self::ALL.len()]
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Orientation {
    #[default]
    Portrait,
    Landscape,
}

impl Orientation {
    pub fn label(self) -> &'static str {
        match self {
            Orientation::Portrait => "Portrait",
            Orientation::Landscape => "Landscape",
        }
    }

    pub fn toggled(self) -> Self {
        match self {
            Orientation::Portrait => Orientation::Landscape,
            Orientation::Landscape => Orientation::Portrait,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(default)]
pub struct SheetSettings {
    pub paper: Paper,
    pub orientation: Orientation,
    pub columns: u8,
    pub rows: u8,
    pub margin_mm: f32,
    pub gutter_mm: f32,
    pub captions: bool,
    pub index: bool,
    pub rotate: bool,
    pub header: bool,
    pub footer: bool,
    /// Blank means "use the project name".
    pub title: String,
}

impl Default for SheetSettings {
    fn default() -> Self {
        Self {
            paper: Paper::A4,
            orientation: Orientation::Portrait,
            columns: 3,
            rows: 4,
            margin_mm: 12.0,
            gutter_mm: 4.0,
            captions: true,
            index: true,
            rotate: false,
            header: true,
            footer: true,
            title: String::new(),
        }
    }
}

impl SheetSettings {
    pub fn normalize(&mut self) {
        self.columns = self.columns.clamp(1, 12);
        self.rows = self.rows.clamp(1, 12);
        self.margin_mm = self.margin_mm.clamp(2.0, 40.0);
        self.gutter_mm = self.gutter_mm.clamp(0.0, 20.0);
        if !self.margin_mm.is_finite() {
            self.margin_mm = 12.0;
        }
        if !self.gutter_mm.is_finite() {
            self.gutter_mm = 4.0;
        }
    }

    pub fn title_or<'a>(&'a self, project_name: &'a str) -> &'a str {
        let title = self.title.trim();
        if title.is_empty() {
            project_name
        } else {
            title
        }
    }

    pub fn per_page(&self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }

    pub fn paper_mm(&self) -> (f32, f32) {
        let (width, height) = self.paper.mm();
        match self.orientation {
            Orientation::Portrait => (width, height),
            Orientation::Landscape => (height, width),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SavedSheet {
    pub id: SheetId,
    pub name: String,
    pub settings: SheetSettings,
    #[serde(default)]
    pub photo_ids: Vec<PhotoId>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Project {
    pub schema: u32,
    pub name: String,
    pub photos: Vec<Photo>,
    pub sheet: SheetSettings,
    pub saved_sheets: Vec<SavedSheet>,
    pub next_photo_id: u64,
    pub next_sheet_id: u64,
}

impl Default for Project {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

impl Project {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            name: name.into(),
            photos: Vec::new(),
            sheet: SheetSettings::default(),
            saved_sheets: Vec::new(),
            next_photo_id: 1,
            next_sheet_id: 1,
        }
    }

    pub fn allocate_photo_id(&mut self) -> PhotoId {
        let id = PhotoId(self.next_photo_id.max(1));
        self.next_photo_id = id.0 + 1;
        id
    }

    pub fn add_photo(&mut self, path: PathBuf, width: u32, height: u32, bytes: u64) -> PhotoId {
        let id = self.allocate_photo_id();
        self.photos.push(Photo {
            id,
            path,
            width,
            height,
            bytes,
            tags: Vec::new(),
            rotation: 0,
        });
        id
    }

    pub fn find_by_path(&self, path: &Path) -> Option<&Photo> {
        self.photos.iter().find(|photo| photo.path == path)
    }

    pub fn photo(&self, id: PhotoId) -> Option<&Photo> {
        self.photos.iter().find(|photo| photo.id == id)
    }

    pub fn photo_mut(&mut self, id: PhotoId) -> Option<&mut Photo> {
        self.photos.iter_mut().find(|photo| photo.id == id)
    }

    pub fn remove_photos(&mut self, ids: &[PhotoId]) -> usize {
        let before = self.photos.len();
        let ids: std::collections::HashSet<PhotoId> = ids.iter().copied().collect();
        self.photos.retain(|photo| !ids.contains(&photo.id));
        self.saved_sheets
            .iter_mut()
            .for_each(|sheet| sheet.photo_ids.retain(|id| !ids.contains(id)));
        before - self.photos.len()
    }

    pub fn tag_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for photo in &self.photos {
            for tag in &photo.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    pub fn untagged_count(&self) -> usize {
        self.photos
            .iter()
            .filter(|photo| photo.tags.is_empty())
            .count()
    }

    pub fn filter_indices(&self, filter: &Filter, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        self.photos
            .iter()
            .enumerate()
            .filter(|(_, photo)| match filter {
                Filter::All => true,
                Filter::Untagged => photo.tags.is_empty(),
                Filter::Tag(tag) => photo.has_tag(tag),
            })
            .filter(|(_, photo)| {
                needle.is_empty() || photo.file_name().to_lowercase().contains(&needle)
            })
            .map(|(index, _)| index)
            .collect()
    }

    pub fn upsert_saved_sheet(
        &mut self,
        name: String,
        settings: SheetSettings,
        photo_ids: Vec<PhotoId>,
    ) -> SheetId {
        let existing = self
            .saved_sheets
            .iter()
            .position(|sheet| sheet.name.eq_ignore_ascii_case(&name));
        match existing {
            Some(index) => {
                let sheet = &mut self.saved_sheets[index];
                sheet.settings = settings;
                sheet.photo_ids = photo_ids;
                sheet.id
            }
            None => {
                let id = SheetId(self.next_sheet_id.max(1));
                self.next_sheet_id = id.0 + 1;
                self.saved_sheets.push(SavedSheet {
                    id,
                    name,
                    settings,
                    photo_ids,
                });
                id
            }
        }
    }

    pub fn normalize(&mut self) {
        if self.name.trim().is_empty() {
            self.name = "Untitled".to_string();
        }
        self.schema = if self.schema == 0 {
            SCHEMA_VERSION
        } else {
            self.schema.min(SCHEMA_VERSION)
        };
        self.sheet.normalize();
        for sheet in &mut self.saved_sheets {
            sheet.settings.normalize();
        }
        for photo in &mut self.photos {
            photo.rotation %= 4;
        }
        self.next_photo_id = self.next_photo_id.max(
            self.photos
                .iter()
                .map(|photo| photo.id.0 + 1)
                .max()
                .unwrap_or(1),
        );
        self.next_sheet_id = self.next_sheet_id.max(
            self.saved_sheets
                .iter()
                .map(|sheet| sheet.id.0 + 1)
                .max()
                .unwrap_or(1),
        );
    }

    pub fn photo_ids(&self) -> Vec<PhotoId> {
        self.photos.iter().map(|photo| photo.id).collect()
    }

    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let data = std::fs::read_to_string(path)?;
        let mut project: Project = serde_json::from_str(&data)?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        for photo in &mut project.photos {
            if photo.path.is_relative() {
                photo.path = base.join(&photo.path);
            }
        }
        if project.schema > SCHEMA_VERSION {
            anyhow::bail!(
                "project uses schema {} but this build supports {}",
                project.schema,
                SCHEMA_VERSION
            );
        }
        project.normalize();
        Ok(project)
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let data = serde_json::to_string_pretty(self)?;
        std::fs::write(path, data + "\n")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    All,
    Untagged,
    Tag(String),
}

impl Filter {
    pub fn matches(&self, photo: &Photo) -> bool {
        match self {
            Filter::All => true,
            Filter::Untagged => photo.tags.is_empty(),
            Filter::Tag(tag) => photo.has_tag(tag),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn project_with_photos() -> Project {
        let mut project = Project::new("Test");
        project.add_photo(PathBuf::from("/tmp/a.jpg"), 4000, 3000, 1_000);
        project.add_photo(PathBuf::from("/tmp/b.jpg"), 3000, 4000, 2_000);
        project
    }

    #[test]
    fn ids_are_stable_and_monotonic() {
        let mut project = Project::new("Test");
        let first = project.add_photo(PathBuf::from("/tmp/a.jpg"), 1, 1, 0);
        let second = project.add_photo(PathBuf::from("/tmp/b.jpg"), 1, 1, 0);
        assert!(second.0 > first.0);
        project.remove_photos(&[second]);
        let third = project.add_photo(PathBuf::from("/tmp/c.jpg"), 1, 1, 0);
        assert!(third.0 > second.0, "removed ids are never reused");
    }

    #[test]
    fn project_roundtrips_with_tags_and_saved_sheets() {
        let mut project = project_with_photos();
        let id = project.photos[0].id;
        project.photos[0].tags.push("street".into());
        project.photos[1].tags.push("street".into());
        project.photos[0].rotation = 1;
        project.sheet.columns = 5;
        project.upsert_saved_sheet("street".into(), project.sheet.clone(), vec![id]);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.contactsheet");
        project.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();

        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded.photos.len(), 2);
        assert_eq!(loaded.sheet.columns, 5);
        assert_eq!(loaded.saved_sheets.len(), 1);
        assert_eq!(loaded.saved_sheets[0].photo_ids, vec![id]);
        assert_eq!(loaded.photos[0].rotation, 1);
    }

    #[test]
    fn rotation_swaps_the_display_aspect() {
        let mut project = project_with_photos();
        let photo = &mut project.photos[0];
        assert!(photo.aspect() > 1.0);
        photo.rotate_clockwise();
        assert!(photo.aspect() < 1.0);
        assert_eq!(photo.oriented_size(), (3000, 4000));
        for _ in 0..4 {
            photo.rotate_clockwise();
        }
        assert_eq!(photo.rotation(), 1);
    }

    #[test]
    fn tag_counts_are_sorted() {
        let mut project = project_with_photos();
        project.photos[0].tags = vec!["zebra".into(), "alpha".into()];
        project.photos[1].tags = vec!["alpha".into()];
        let counts = project.tag_counts();
        assert_eq!(
            counts.keys().cloned().collect::<Vec<_>>(),
            vec!["alpha", "zebra"]
        );
        assert_eq!(counts["alpha"], 2);
        assert_eq!(project.untagged_count(), 0);
    }

    #[test]
    fn filter_and_search_compose() {
        let mut project = project_with_photos();
        project.photos[0].tags.push("street".into());
        let indices = project.filter_indices(&Filter::Tag("street".into()), "");
        assert_eq!(indices, vec![0]);
        let indices = project.filter_indices(&Filter::All, "B.");
        assert_eq!(indices, vec![1]);
    }

    #[test]
    fn saved_sheet_with_same_name_updates_in_place() {
        let mut project = project_with_photos();
        let first = project.upsert_saved_sheet("one".into(), SheetSettings::default(), vec![]);
        let second = project.upsert_saved_sheet("ONE".into(), SheetSettings::default(), vec![]);
        assert_eq!(first, second);
        assert_eq!(project.saved_sheets.len(), 1);
    }

    #[test]
    fn older_project_json_uses_defaults_for_missing_fields() {
        let json = r#"{
            "name": "Legacy",
            "photos": [],
            "sheet": { "columns": 2, "rows": 2 }
        }"#;
        let project: Project = serde_json::from_str(json).unwrap();
        assert_eq!(project.schema, SCHEMA_VERSION);
        assert_eq!(project.sheet.columns, 2);
        assert_eq!(project.sheet.rows, 2);
        assert_eq!(project.sheet.paper, Paper::A4);
        assert!(project.sheet.captions);
        assert_eq!(project.photo_ids().len(), 0);
    }

    #[test]
    fn removing_photos_prunes_saved_sheet_ids() {
        let mut project = project_with_photos();
        let ids = project.photo_ids();
        project.upsert_saved_sheet("one".into(), SheetSettings::default(), ids.clone());
        project.remove_photos(&[ids[0]]);
        assert_eq!(project.saved_sheets[0].photo_ids, vec![ids[1]]);
    }

    #[test]
    fn relative_photo_paths_resolve_against_the_project_file() {
        let directory = tempfile::tempdir().unwrap();
        let project_path = directory.path().join("relative.contactsheet");
        let json = r#"{
            "name": "Relative",
            "photos": [
                { "id": 1, "path": "photos/a.jpg", "width": 10, "height": 10 }
            ],
            "sheet": {}
        }"#;
        std::fs::write(&project_path, json).unwrap();
        let project = Project::load(&project_path).unwrap();
        assert_eq!(
            project.photos[0].path,
            directory.path().join("photos/a.jpg")
        );
    }

    #[test]
    fn newer_schema_is_rejected() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("future.contactsheet");
        let mut project = Project::new("Future");
        project.schema = SCHEMA_VERSION + 1;
        project.save(&path).unwrap();
        assert!(Project::load(&path).is_err());
    }
}
