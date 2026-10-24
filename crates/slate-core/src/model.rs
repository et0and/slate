//! Project data: photos, sheet settings and saved sheets.
//!
//! Everything in this module serializes to the on-disk `.slate` file,
//! so a project can always be reopened and exported later.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::io::{BufReader, BufWriter, Write};
use std::num::NonZeroU32;
use std::ops::{Deref, Index};
use std::path::{Path, PathBuf};
use std::sync::Arc;

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

/// Clockwise quarter turns, always in `0..4`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(try_from = "u8", into = "u8")]
pub struct QuarterTurns(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidQuarterTurns(pub u8);

impl std::fmt::Display for InvalidQuarterTurns {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "rotation must be 0, 1, 2, or 3, got {}", self.0)
    }
}

impl std::error::Error for InvalidQuarterTurns {}

impl QuarterTurns {
    pub const NONE: Self = Self(0);

    pub const fn turns(self) -> u8 {
        self.0
    }

    pub const fn degrees(self) -> u16 {
        self.0 as u16 * 90
    }

    pub const fn is_odd(self) -> bool {
        self.0 % 2 == 1
    }

    pub const fn clockwise(self) -> Self {
        Self((self.0 + 1) % 4)
    }
}

impl TryFrom<u8> for QuarterTurns {
    type Error = InvalidQuarterTurns;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value < 4 {
            Ok(Self(value))
        } else {
            Err(InvalidQuarterTurns(value))
        }
    }
}

impl From<QuarterTurns> for u8 {
    fn from(value: QuarterTurns) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Dimensions {
    width: NonZeroU32,
    height: NonZeroU32,
}

impl Dimensions {
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }

    pub const fn width(self) -> u32 {
        self.width.get()
    }

    pub const fn height(self) -> u32 {
        self.height.get()
    }

    pub fn aspect(self) -> f32 {
        self.width.get() as f32 / self.height.get() as f32
    }

    pub fn oriented(self, turns: QuarterTurns) -> Self {
        if turns.is_odd() {
            Self {
                width: self.height,
                height: self.width,
            }
        } else {
            self
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum PhotoSize {
    #[default]
    Unknown,
    Known(Dimensions),
}

impl PhotoSize {
    pub fn new(width: u32, height: u32) -> Self {
        Dimensions::new(width, height).map_or(Self::Unknown, Self::Known)
    }

    pub fn dimensions(self) -> Option<Dimensions> {
        match self {
            Self::Known(dimensions) => Some(dimensions),
            Self::Unknown => None,
        }
    }
}

impl Serialize for PhotoSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        #[derive(Serialize)]
        struct Wire {
            width: u32,
            height: u32,
        }
        let (width, height) = match self.dimensions() {
            Some(dimensions) => (dimensions.width(), dimensions.height()),
            None => (0, 0),
        };
        Wire { width, height }.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PhotoSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct Wire {
            #[serde(default)]
            width: u32,
            #[serde(default)]
            height: u32,
        }
        let wire = Wire::deserialize(deserializer)?;
        Ok(Self::new(wire.width, wire.height))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Photo {
    id: PhotoId,
    pub path: PathBuf,
    #[serde(flatten)]
    pub size: PhotoSize,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub tags: Vec<String>,
    /// User rotation in clockwise quarter turns.
    #[serde(default)]
    pub rotation: QuarterTurns,
}

impl Photo {
    pub fn new(id: PhotoId, path: PathBuf, size: PhotoSize, bytes: u64) -> Self {
        Self {
            id,
            path,
            size,
            bytes,
            tags: Vec::new(),
            rotation: QuarterTurns::NONE,
        }
    }

    pub fn id(&self) -> PhotoId {
        self.id
    }

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

    pub fn rotation(&self) -> QuarterTurns {
        self.rotation
    }

    pub fn rotate_clockwise(&mut self) {
        self.rotation = self.rotation.clockwise();
    }

    pub fn oriented_size(&self) -> Option<(u32, u32)> {
        let dimensions = self.size.dimensions()?.oriented(self.rotation());
        Some((dimensions.width(), dimensions.height()))
    }

    pub fn aspect(&self) -> Option<f32> {
        Some(self.size.dimensions()?.oriented(self.rotation()).aspect())
    }

    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|existing| existing == tag)
    }
}

#[derive(Debug, Clone, Default)]
struct PhotoData {
    photos: Vec<Photo>,
    index: HashMap<PhotoId, usize>,
}

#[derive(Debug, Clone, Default)]
pub struct PhotoSet {
    data: Arc<PhotoData>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DuplicatePhotoId(pub PhotoId);

impl std::fmt::Display for DuplicatePhotoId {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "duplicate photo id {}", self.0.0)
    }
}

impl std::error::Error for DuplicatePhotoId {}

impl PhotoSet {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_vec(photos: Vec<Photo>) -> Result<Self, DuplicatePhotoId> {
        let capacity = photos.len();
        let mut data = PhotoData {
            photos,
            index: HashMap::with_capacity(capacity),
        };
        for (position, photo) in data.photos.iter().enumerate() {
            if data.index.insert(photo.id, position).is_some() {
                return Err(DuplicatePhotoId(photo.id));
            }
        }
        Ok(Self {
            data: Arc::new(data),
        })
    }

    pub fn push(&mut self, photo: Photo) -> Result<(), DuplicatePhotoId> {
        if self.data.index.contains_key(&photo.id) {
            return Err(DuplicatePhotoId(photo.id));
        }
        let data = Arc::make_mut(&mut self.data);
        data.index.insert(photo.id, data.photos.len());
        data.photos.push(photo);
        Ok(())
    }

    pub fn photo(&self, id: PhotoId) -> Option<&Photo> {
        self.data
            .index
            .get(&id)
            .and_then(|position| self.data.photos.get(*position))
    }

    pub fn photo_mut(&mut self, id: PhotoId) -> Option<&mut Photo> {
        let position = *self.data.index.get(&id)?;
        let data = Arc::make_mut(&mut self.data);
        data.photos.get_mut(position)
    }

    pub fn position(&self, id: PhotoId) -> Option<usize> {
        self.data.index.get(&id).copied()
    }

    pub fn shares_data_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    pub fn remove_ids(&mut self, ids: &HashSet<PhotoId>) -> usize {
        let data = Arc::make_mut(&mut self.data);
        let before = data.photos.len();
        data.photos.retain(|photo| !ids.contains(&photo.id));
        data.rebuild_index();
        before - data.photos.len()
    }
}

impl PhotoData {
    fn rebuild_index(&mut self) {
        self.index.clear();
        self.index.reserve(self.photos.len());
        for (position, photo) in self.photos.iter().enumerate() {
            self.index.insert(photo.id, position);
        }
    }
}

impl Deref for PhotoSet {
    type Target = [Photo];

    fn deref(&self) -> &Self::Target {
        &self.data.photos
    }
}

impl Index<usize> for PhotoSet {
    type Output = Photo;

    fn index(&self, index: usize) -> &Self::Output {
        &self.data.photos[index]
    }
}

impl<'a> IntoIterator for &'a PhotoSet {
    type Item = &'a Photo;
    type IntoIter = std::slice::Iter<'a, Photo>;

    fn into_iter(self) -> Self::IntoIter {
        self.data.photos.iter()
    }
}

impl<'a> IntoIterator for &'a mut PhotoSet {
    type Item = &'a mut Photo;
    type IntoIter = std::slice::IterMut<'a, Photo>;

    fn into_iter(self) -> Self::IntoIter {
        Arc::make_mut(&mut self.data).photos.iter_mut()
    }
}

impl Serialize for PhotoSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.data.photos.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for PhotoSet {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let photos = Vec::<Photo>::deserialize(deserializer)?;
        Self::from_vec(photos).map_err(serde::de::Error::custom)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum Paper {
    #[default]
    A4,
    A3,
    Letter,
    Tabloid,
    Custom,
}

impl Paper {
    pub const ALL: [Paper; 5] = [
        Paper::A4,
        Paper::A3,
        Paper::Letter,
        Paper::Tabloid,
        Paper::Custom,
    ];

    /// Portrait width and height for a fixed paper size, in millimetres.
    /// Custom paper is resolved through [`SheetSettings::paper_mm`].
    pub fn mm(self) -> (f32, f32) {
        match self {
            Paper::A4 => (210.0, 297.0),
            Paper::A3 => (297.0, 420.0),
            Paper::Letter => (215.9, 279.4),
            Paper::Tabloid => (279.4, 431.8),
            Paper::Custom => (210.0, 297.0),
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Paper::A4 => "A4",
            Paper::A3 => "A3",
            Paper::Letter => "Letter",
            Paper::Tabloid => "Tabloid",
            Paper::Custom => "Custom",
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
    /// Used when `paper` is [`Paper::Custom`].
    pub custom_paper_width_mm: f32,
    /// Used when `paper` is [`Paper::Custom`].
    pub custom_paper_height_mm: f32,
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
            custom_paper_width_mm: 210.0,
            custom_paper_height_mm: 297.0,
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
        if !self.custom_paper_width_mm.is_finite() {
            self.custom_paper_width_mm = 210.0;
        }
        if !self.custom_paper_height_mm.is_finite() {
            self.custom_paper_height_mm = 297.0;
        }
        self.custom_paper_width_mm = self.custom_paper_width_mm.clamp(50.0, 2000.0);
        self.custom_paper_height_mm = self.custom_paper_height_mm.clamp(50.0, 2000.0);
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
        let (width, height) = if self.paper == Paper::Custom {
            (self.custom_paper_width_mm, self.custom_paper_height_mm)
        } else {
            self.paper.mm()
        };
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
    pub photos: PhotoSet,
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
            photos: PhotoSet::new(),
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
        self.photos
            .push(Photo::new(id, path, PhotoSize::new(width, height), bytes))
            .expect("allocated photo ids are unique");
        id
    }

    pub fn photo(&self, id: PhotoId) -> Option<&Photo> {
        self.photos.photo(id)
    }

    pub fn photo_mut(&mut self, id: PhotoId) -> Option<&mut Photo> {
        self.photos.photo_mut(id)
    }

    pub fn remove_photos(&mut self, ids: &[PhotoId]) -> usize {
        let ids: HashSet<PhotoId> = ids.iter().copied().collect();
        let removed = self.photos.remove_ids(&ids);
        self.saved_sheets
            .iter_mut()
            .for_each(|sheet| sheet.photo_ids.retain(|id| !ids.contains(id)));
        removed
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
        let file = std::fs::File::open(path)?;
        let mut project: Project = serde_json::from_reader(BufReader::new(file))?;
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
        let file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, self)?;
        writer.write_all(b"\n")?;
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
    fn custom_paper_sizes_are_clamped() {
        let mut settings = SheetSettings {
            paper: Paper::Custom,
            custom_paper_width_mm: 2.0,
            custom_paper_height_mm: 9_000.0,
            ..Default::default()
        };
        settings.normalize();
        assert_eq!(settings.custom_paper_width_mm, 50.0);
        assert_eq!(settings.custom_paper_height_mm, 2000.0);
    }

    #[test]
    fn quarter_turns_reject_invalid_values() {
        assert!(QuarterTurns::try_from(4).is_err());
        assert!(serde_json::from_str::<QuarterTurns>("4").is_err());
        assert_eq!(QuarterTurns::try_from(3).unwrap().clockwise().turns(), 0);
        assert_eq!(QuarterTurns::try_from(2).unwrap().degrees(), 180);
    }

    #[test]
    fn photo_size_keeps_unknown_distinct_from_zero_pixels() {
        assert_eq!(PhotoSize::new(0, 10), PhotoSize::Unknown);
        assert_eq!(PhotoSize::new(10, 0), PhotoSize::Unknown);
        let dimensions = Dimensions::new(4000, 3000).unwrap();
        assert_eq!(dimensions.width(), 4000);
        assert_eq!(
            dimensions
                .oriented(QuarterTurns::try_from(1).unwrap())
                .width(),
            3000
        );
    }

    #[test]
    fn unknown_dimensions_survive_a_project_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("unknown.slate");
        let mut project = Project::new("Test");
        let id = project.add_photo(PathBuf::from("/tmp/missing.jpg"), 0, 0, 1);
        project.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();
        assert_eq!(loaded.photo(id).unwrap().size, PhotoSize::Unknown);
        assert_eq!(loaded.photo(id).unwrap().oriented_size(), None);
    }

    #[test]
    fn photo_set_rejects_duplicate_ids() {
        let photos = project_with_photos();
        let duplicate = photos.photos[0].clone();
        assert!(PhotoSet::from_vec(vec![duplicate.clone(), duplicate]).is_err());
    }

    #[test]
    fn photo_set_lookup_survives_removal() {
        let mut project = project_with_photos();
        let first = project.photos[0].id;
        let second = project.photos[1].id;
        project.remove_photos(&[first]);
        assert!(project.photo(first).is_none());
        assert_eq!(project.photo(second).map(|photo| photo.id), Some(second));
        assert_eq!(project.photos.position(second), Some(0));
    }

    #[test]
    fn project_clone_shares_photo_data_until_a_mutation() {
        let mut project = project_with_photos();
        let clone = project.clone();
        assert!(Arc::ptr_eq(&project.photos.data, &clone.photos.data));
        project
            .photo_mut(PhotoId(1))
            .unwrap()
            .tags
            .push("new".into());
        assert!(!Arc::ptr_eq(&project.photos.data, &clone.photos.data));
        assert!(clone.photo(PhotoId(1)).unwrap().tags.is_empty());
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
        project
            .photo_mut(PhotoId(1))
            .unwrap()
            .tags
            .push("street".into());
        project
            .photo_mut(PhotoId(2))
            .unwrap()
            .tags
            .push("street".into());
        project.photo_mut(PhotoId(1)).unwrap().rotation = QuarterTurns::try_from(1).unwrap();
        project.sheet.columns = 5;
        project.sheet.paper = Paper::Custom;
        project.sheet.custom_paper_width_mm = 120.0;
        project.sheet.custom_paper_height_mm = 180.0;
        project.upsert_saved_sheet("street".into(), project.sheet.clone(), vec![id]);

        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("test.slate");
        project.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();

        assert_eq!(loaded.name, "Test");
        assert_eq!(loaded.photos.len(), 2);
        assert_eq!(loaded.sheet.columns, 5);
        assert_eq!(loaded.sheet.paper, Paper::Custom);
        assert_eq!(loaded.sheet.custom_paper_width_mm, 120.0);
        assert_eq!(loaded.sheet.custom_paper_height_mm, 180.0);
        assert_eq!(loaded.saved_sheets.len(), 1);
        assert_eq!(loaded.saved_sheets[0].photo_ids, vec![id]);
        assert_eq!(loaded.photos[0].rotation.turns(), 1);
    }

    #[test]
    fn rotation_swaps_the_display_aspect() {
        let mut project = project_with_photos();
        let photo = project.photo_mut(PhotoId(1)).unwrap();
        assert!(photo.aspect().unwrap() > 1.0);
        photo.rotate_clockwise();
        assert!(photo.aspect().unwrap() < 1.0);
        assert_eq!(photo.oriented_size(), Some((3000, 4000)));
        for _ in 0..4 {
            photo.rotate_clockwise();
        }
        assert_eq!(photo.rotation().turns(), 1);
    }

    #[test]
    fn tag_counts_are_sorted() {
        let mut project = project_with_photos();
        project.photo_mut(PhotoId(1)).unwrap().tags = vec!["zebra".into(), "alpha".into()];
        project.photo_mut(PhotoId(2)).unwrap().tags = vec!["alpha".into()];
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
        project
            .photo_mut(PhotoId(1))
            .unwrap()
            .tags
            .push("street".into());
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
        assert_eq!(project.sheet.custom_paper_width_mm, 210.0);
        assert_eq!(project.sheet.custom_paper_height_mm, 297.0);
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
        let project_path = directory.path().join("relative.slate");
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
        let path = directory.path().join("future.slate");
        let mut project = Project::new("Future");
        project.schema = SCHEMA_VERSION + 1;
        project.save(&path).unwrap();
        assert!(Project::load(&path).is_err());
    }
}
