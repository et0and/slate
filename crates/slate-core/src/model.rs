//! Project data: photos, sheet settings and saved sheets.
//!
//! Everything in this module serializes to the on-disk `.slate` file,
//! so a project can always be reopened and exported later.

use crate::adjust::AdjustmentMap;
use std::collections::{BTreeMap, HashSet};
use std::io::{BufReader, BufWriter, Write};
use std::num::NonZeroU32;
use std::ops::Index;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use im::{HashMap as ImHashMap, Vector as ImVector};
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

/// Zero to five stars. Zero means unrated.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(try_from = "u8", into = "u8")]
pub struct StarRating(u8);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InvalidStarRating(pub u8);

impl std::fmt::Display for InvalidStarRating {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "rating must be 0..=5, got {}", self.0)
    }
}

impl std::error::Error for InvalidStarRating {}

impl StarRating {
    pub const NONE: Self = Self(0);
    pub const MAX: u8 = 5;

    #[must_use]
    pub const fn get(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn is_rated(self) -> bool {
        self.0 > 0
    }

    #[must_use]
    pub fn clamped(value: u8) -> Self {
        Self(value.min(Self::MAX))
    }
}

impl TryFrom<u8> for StarRating {
    type Error = InvalidStarRating;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        if value <= Self::MAX {
            Ok(Self(value))
        } else {
            Err(InvalidStarRating(value))
        }
    }
}

impl From<StarRating> for u8 {
    fn from(value: StarRating) -> Self {
        value.0
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
#[serde(rename_all = "lowercase")]
#[repr(u8)]
pub enum ColorLabel {
    #[default]
    None,
    Red,
    Yellow,
    Green,
    Blue,
    Purple,
}

impl ColorLabel {
    pub const ALL: [Self; 6] = [
        Self::None,
        Self::Red,
        Self::Yellow,
        Self::Green,
        Self::Blue,
        Self::Purple,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::None => "None",
            Self::Red => "Red",
            Self::Yellow => "Yellow",
            Self::Green => "Green",
            Self::Blue => "Blue",
            Self::Purple => "Purple",
        }
    }

    #[must_use]
    pub const fn is_set(self) -> bool {
        !matches!(self, Self::None)
    }
}

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

    #[must_use]
    pub const fn turns(self) -> u8 {
        self.0
    }

    #[must_use]
    pub const fn degrees(self) -> u16 {
        self.0 as u16 * 90
    }

    #[must_use]
    pub const fn is_odd(self) -> bool {
        self.0 % 2 == 1
    }

    #[must_use]
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
    #[must_use]
    pub fn new(width: u32, height: u32) -> Option<Self> {
        Some(Self {
            width: NonZeroU32::new(width)?,
            height: NonZeroU32::new(height)?,
        })
    }

    #[must_use]
    pub const fn width(self) -> u32 {
        self.width.get()
    }

    #[must_use]
    pub const fn height(self) -> u32 {
        self.height.get()
    }

    #[must_use]
    pub fn aspect(self) -> f32 {
        self.width.get() as f32 / self.height.get() as f32
    }

    #[must_use]
    pub const fn oriented(self, turns: QuarterTurns) -> Self {
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

    #[must_use]
    pub const fn dimensions(self) -> Option<Dimensions> {
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
        let (width, height) = self.dimensions().map_or((0, 0), |dimensions| {
            (dimensions.width(), dimensions.height())
        });
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
    #[serde(with = "path_arc")]
    pub path: Arc<Path>,
    #[serde(flatten)]
    pub size: PhotoSize,
    #[serde(default)]
    pub bytes: u64,
    #[serde(default)]
    pub tags: Box<[String]>,
    /// User rotation in clockwise quarter turns.
    #[serde(default)]
    pub rotation: QuarterTurns,
    /// Star rating. Zero means unrated.
    #[serde(default)]
    pub rating: StarRating,
    /// Colour label used for culling.
    #[serde(default)]
    pub label: ColorLabel,
    /// Camera capture time as `YYYY-MM-DD HH:MM:SS`, falling back to the
    /// file's modified time for images without EXIF metadata.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub captured_at: Option<Box<str>>,
}

mod path_arc {
    use std::path::{Path, PathBuf};
    use std::sync::Arc;

    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S>(path: &Arc<Path>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        path.as_ref().to_string_lossy().serialize(serializer)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Arc<Path>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let path = PathBuf::deserialize(deserializer)?;
        Ok(Arc::from(path))
    }
}

impl Photo {
    #[must_use]
    pub fn new(id: PhotoId, path: PathBuf, size: PhotoSize, bytes: u64) -> Self {
        Self {
            id,
            path: Arc::from(path),
            size,
            bytes,
            tags: Box::default(),
            rotation: QuarterTurns::NONE,
            rating: StarRating::NONE,
            label: ColorLabel::None,
            captured_at: None,
        }
    }

    pub fn set_path(&mut self, path: PathBuf) {
        self.path = Arc::from(path);
    }

    pub fn set_tags(&mut self, tags: Vec<String>) {
        self.tags = tags.into_boxed_slice();
    }

    pub fn push_tag(&mut self, tag: String) {
        let mut tags = std::mem::take(&mut self.tags).into_vec();
        tags.push(tag);
        tags.sort();
        self.tags = tags.into_boxed_slice();
    }

    pub fn remove_tag(&mut self, tag: &str) -> bool {
        let mut tags = std::mem::take(&mut self.tags).into_vec();
        let before = tags.len();
        tags.retain(|existing| existing != tag);
        self.tags = tags.into_boxed_slice();
        before != self.tags.len()
    }

    pub fn clear_tags(&mut self) -> bool {
        let had_tags = !self.tags.is_empty();
        self.tags = Box::default();
        had_tags
    }

    #[must_use]
    pub const fn id(&self) -> PhotoId {
        self.id
    }

    #[must_use]
    pub fn file_name(&self) -> String {
        self.path.file_name().map_or_else(
            || self.path.to_string_lossy().into_owned(),
            |name| name.to_string_lossy().into_owned(),
        )
    }

    #[must_use]
    pub fn extension(&self) -> String {
        self.path
            .extension()
            .map(|ext| ext.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default()
    }

    #[must_use]
    pub const fn rotation(&self) -> QuarterTurns {
        self.rotation
    }

    pub const fn rotate_clockwise(&mut self) {
        self.rotation = self.rotation.clockwise();
    }

    #[must_use]
    pub const fn rating(&self) -> StarRating {
        self.rating
    }

    pub const fn set_rating(&mut self, rating: StarRating) {
        self.rating = rating;
    }

    #[must_use]
    pub const fn label(&self) -> ColorLabel {
        self.label
    }

    pub const fn set_label(&mut self, label: ColorLabel) {
        self.label = label;
    }

    #[must_use]
    pub fn oriented_size(&self) -> Option<(u32, u32)> {
        let dimensions = self.size.dimensions()?.oriented(self.rotation());
        Some((dimensions.width(), dimensions.height()))
    }

    #[must_use]
    pub fn aspect(&self) -> Option<f32> {
        Some(self.size.dimensions()?.oriented(self.rotation()).aspect())
    }

    #[must_use]
    pub fn has_tag(&self, tag: &str) -> bool {
        self.tags.iter().any(|existing| existing == tag)
    }
}

const PHOTO_CHUNK: usize = 1024;
const EMPTY_POSITION: u32 = u32::MAX;

#[derive(Debug, Clone)]
enum PositionIndex {
    Dense(ImVector<u32>),
    Sparse(ImHashMap<PhotoId, usize>),
}

impl Default for PositionIndex {
    fn default() -> Self {
        Self::Dense(ImVector::new())
    }
}

impl PositionIndex {
    fn new(max_id: u64, count: usize) -> Self {
        if u32::try_from(max_id).is_ok()
            && max_id as usize <= count.saturating_mul(4).saturating_add(1024)
        {
            Self::Dense(ImVector::new())
        } else {
            Self::Sparse(ImHashMap::new())
        }
    }

    fn get(&self, id: PhotoId) -> Option<usize> {
        match self {
            Self::Dense(positions) => positions
                .get(id.0 as usize)
                .filter(|position| **position != EMPTY_POSITION)
                .map(|position| *position as usize),
            Self::Sparse(positions) => positions.get(&id).copied(),
        }
    }

    fn insert(&mut self, id: PhotoId, position: usize) {
        match self {
            Self::Dense(positions) => {
                let index = id.0 as usize;
                while positions.len() <= index {
                    positions.push_back(EMPTY_POSITION);
                }
                positions.set(index, position as u32);
            }
            Self::Sparse(positions) => {
                positions.insert(id, position);
            }
        }
    }

    fn needs_sparse(&self, id: PhotoId) -> bool {
        match self {
            Self::Dense(positions) => {
                id.0 > u64::from(u32::MAX)
                    || id.0 as usize > positions.len().saturating_mul(4).saturating_add(1024)
            }
            Self::Sparse(_) => false,
        }
    }
}

#[derive(Debug, Clone, Default)]
struct PhotoData {
    chunks: Vec<Arc<Vec<Photo>>>,
    /// Flat start position for each chunk. Chunks can become shorter than
    /// `PHOTO_CHUNK` after a removal, so the flat index cannot be derived.
    offsets: Vec<usize>,
    index: PositionIndex,
    len: usize,
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
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn from_vec(photos: Vec<Photo>) -> Result<Self, DuplicatePhotoId> {
        let count = photos.len();
        let max_id = photos.iter().map(|photo| photo.id.0).max().unwrap_or(0);
        let mut data = PhotoData {
            chunks: Vec::with_capacity(count.div_ceil(PHOTO_CHUNK)),
            offsets: Vec::with_capacity(count.div_ceil(PHOTO_CHUNK)),
            index: PositionIndex::new(max_id, count),
            len: 0,
        };
        for photo in photos {
            if data.index.get(photo.id).is_some() {
                return Err(DuplicatePhotoId(photo.id));
            }
            push_chunked(&mut data, photo);
        }
        Ok(Self {
            data: Arc::new(data),
        })
    }

    pub fn push(&mut self, photo: Photo) -> Result<(), DuplicatePhotoId> {
        if self.data.index.get(photo.id).is_some() {
            return Err(DuplicatePhotoId(photo.id));
        }
        let data = Arc::make_mut(&mut self.data);
        if data.index.needs_sparse(photo.id) {
            data.rebuild_index();
        }
        push_chunked(data, photo);
        Ok(())
    }

    #[must_use]
    pub fn get(&self, index: usize) -> Option<&Photo> {
        let chunk_index = self
            .data
            .offsets
            .partition_point(|start| *start <= index)
            .checked_sub(1)?;
        let slot = index - self.data.offsets[chunk_index];
        self.data.chunks.get(chunk_index)?.get(slot)
    }

    #[must_use]
    pub fn iter(&self) -> PhotoIter<'_> {
        PhotoIter {
            outer: self.data.chunks.iter(),
            inner: [].iter(),
        }
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.len == 0
    }

    #[must_use]
    pub fn photo(&self, id: PhotoId) -> Option<&Photo> {
        self.data
            .index
            .get(id)
            .and_then(|position| self.get(position))
    }

    pub fn photo_mut(&mut self, id: PhotoId) -> Option<&mut Photo> {
        let position = self.data.index.get(id)?;
        let data = Arc::make_mut(&mut self.data);
        let chunk_index = data
            .offsets
            .partition_point(|start| *start <= position)
            .checked_sub(1)?;
        let slot = position - data.offsets[chunk_index];
        let chunk = data.chunks.get_mut(chunk_index)?;
        Arc::make_mut(chunk).get_mut(slot)
    }

    #[must_use]
    pub fn position(&self, id: PhotoId) -> Option<usize> {
        self.data.index.get(id)
    }

    #[must_use]
    pub fn shares_data_with(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.data, &other.data)
    }

    pub fn remove_ids(&mut self, ids: &HashSet<PhotoId>) -> usize {
        if ids.is_empty() {
            return 0;
        }
        let before = self.data.len;
        let data = Arc::make_mut(&mut self.data);
        let mut chunks = Vec::with_capacity(data.chunks.len());
        for chunk in &data.chunks {
            if chunk.iter().all(|photo| !ids.contains(&photo.id)) {
                chunks.push(Arc::clone(chunk));
                continue;
            }
            let mut next = Vec::with_capacity(chunk.len());
            for photo in chunk.iter() {
                if !ids.contains(&photo.id) {
                    next.push(photo.clone());
                }
            }
            if !next.is_empty() {
                chunks.push(Arc::new(next));
            }
        }
        data.chunks = chunks;
        data.rebuild_index();
        before - data.len
    }
}

fn push_chunked(data: &mut PhotoData, photo: Photo) {
    let id = photo.id;
    let position = data.len;
    let needs_chunk = data
        .chunks
        .last()
        .is_none_or(|chunk| chunk.len() >= PHOTO_CHUNK);
    if needs_chunk {
        data.offsets.push(position);
        data.chunks.push(Arc::new(Vec::with_capacity(PHOTO_CHUNK)));
    }
    let chunk = data.chunks.last_mut().expect("chunk was just inserted");
    Arc::make_mut(chunk).push(photo);
    data.index.insert(id, position);
    data.len += 1;
}

impl PhotoData {
    fn rebuild_index(&mut self) {
        let max_id = self
            .chunks
            .iter()
            .flat_map(|chunk| chunk.iter())
            .map(|photo| photo.id.0)
            .max()
            .unwrap_or(0);
        let count = self.chunks.iter().map(|chunk| chunk.len()).sum();
        self.index = PositionIndex::new(max_id, count);
        self.offsets.clear();
        self.offsets.reserve(self.chunks.len());
        self.len = 0;
        for chunk in &self.chunks {
            self.offsets.push(self.len);
            for photo in chunk.iter() {
                self.index.insert(photo.id, self.len);
                self.len += 1;
            }
        }
    }
}

pub struct PhotoIter<'a> {
    outer: std::slice::Iter<'a, Arc<Vec<Photo>>>,
    inner: std::slice::Iter<'a, Photo>,
}

impl<'a> Iterator for PhotoIter<'a> {
    type Item = &'a Photo;

    fn next(&mut self) -> Option<Self::Item> {
        loop {
            if let Some(photo) = self.inner.next() {
                return Some(photo);
            }
            let chunk = self.outer.next()?;
            self.inner = chunk.iter();
        }
    }
}

impl Index<usize> for PhotoSet {
    type Output = Photo;

    fn index(&self, index: usize) -> &Self::Output {
        self.get(index).expect("photo index is in bounds")
    }
}

impl<'a> IntoIterator for &'a PhotoSet {
    type Item = &'a Photo;
    type IntoIter = PhotoIter<'a>;

    fn into_iter(self) -> Self::IntoIter {
        self.iter()
    }
}

impl Serialize for PhotoSet {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut sequence = serializer.serialize_seq(Some(self.len()))?;
        for photo in self {
            sequence.serialize_element(photo)?;
        }
        sequence.end()
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
    pub const ALL: [Self; 5] = [
        Self::A4,
        Self::A3,
        Self::Letter,
        Self::Tabloid,
        Self::Custom,
    ];

    /// Portrait width and height for a fixed paper size, in millimetres.
    /// Custom paper is resolved through [`SheetSettings::paper_mm`].
    #[must_use]
    pub const fn mm(self) -> (f32, f32) {
        match self {
            Self::A4 | Self::Custom => (210.0, 297.0),
            Self::A3 => (297.0, 420.0),
            Self::Letter => (215.9, 279.4),
            Self::Tabloid => (279.4, 431.8),
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::A4 => "A4",
            Self::A3 => "A3",
            Self::Letter => "Letter",
            Self::Tabloid => "Tabloid",
            Self::Custom => "Custom",
        }
    }

    #[must_use]
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
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Portrait => "Portrait",
            Self::Landscape => "Landscape",
        }
    }

    #[must_use]
    pub const fn toggled(self) -> Self {
        match self {
            Self::Portrait => Self::Landscape,
            Self::Landscape => Self::Portrait,
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

    #[must_use]
    pub fn title_or<'a>(&'a self, project_name: &'a str) -> &'a str {
        let title = self.title.trim();
        if title.is_empty() {
            project_name
        } else {
            title
        }
    }

    #[must_use]
    pub fn per_page(&self) -> usize {
        usize::from(self.columns) * usize::from(self.rows)
    }

    #[must_use]
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
pub struct Collection {
    pub schema: u32,
    pub name: String,
    pub photos: PhotoSet,
    pub sheet: SheetSettings,
    pub saved_sheets: Vec<SavedSheet>,
    /// Non-destructive tone and colour adjustments, only for adjusted photos.
    #[serde(skip_serializing_if = "AdjustmentMap::is_empty")]
    pub adjustments: AdjustmentMap,
    pub next_photo_id: u64,
    pub next_sheet_id: u64,
}

/// Backwards-compatible alias while callers migrate from the old project model.
pub type Project = Collection;

impl Default for Collection {
    fn default() -> Self {
        Self::new("Untitled")
    }
}

impl Collection {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            schema: SCHEMA_VERSION,
            name: name.into(),
            photos: PhotoSet::new(),
            sheet: SheetSettings::default(),
            saved_sheets: Vec::new(),
            adjustments: AdjustmentMap::default(),
            next_photo_id: 1,
            next_sheet_id: 1,
        }
    }

    pub fn allocate_photo_id(&mut self) -> PhotoId {
        let id = PhotoId(self.next_photo_id.max(1));
        self.next_photo_id = id.0 + 1;
        id
    }

    /// Add a photo and return its allocated identifier.
    ///
    /// # Panics
    ///
    /// Panics only if the internal photo set rejects an identifier that was
    /// just allocated by [`Self::allocate_photo_id`].
    pub fn add_photo(&mut self, path: PathBuf, width: u32, height: u32, bytes: u64) -> PhotoId {
        let id = self.allocate_photo_id();
        self.photos
            .push(Photo::new(id, path, PhotoSize::new(width, height), bytes))
            .expect("allocated photo ids are unique");
        id
    }

    #[must_use]
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
        self.adjustments.retain(|id| !ids.contains(&id));
        removed
    }

    #[must_use]
    pub fn tag_counts(&self) -> BTreeMap<String, usize> {
        let mut counts = BTreeMap::new();
        for photo in &self.photos {
            for tag in &photo.tags {
                *counts.entry(tag.clone()).or_insert(0) += 1;
            }
        }
        counts
    }

    #[must_use]
    pub fn untagged_count(&self) -> usize {
        self.photos
            .iter()
            .filter(|photo| photo.tags.is_empty())
            .count()
    }

    #[must_use]
    pub fn filter_indices(&self, filter: &Filter, query: &str) -> Vec<usize> {
        let needle = query.trim().to_lowercase();
        self.photos
            .iter()
            .enumerate()
            .filter(|(_, photo)| filter.matches(photo))
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
        if let Some(index) = existing {
            let sheet = &mut self.saved_sheets[index];
            sheet.settings = settings;
            sheet.photo_ids = photo_ids;
            sheet.id
        } else {
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
        let photos = &self.photos;
        self.adjustments.retain(|id| photos.photo(id).is_some());
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

    #[must_use]
    pub fn photo_ids(&self) -> Vec<PhotoId> {
        self.photos.iter().map(|photo| photo.id).collect()
    }

    #[tracing::instrument(skip_all)]
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let file = std::fs::File::open(path)?;
        let mut project: Self = serde_json::from_reader(BufReader::new(file))?;
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let relative_ids: Vec<PhotoId> = project
            .photos
            .iter()
            .filter(|photo| photo.path.is_relative())
            .map(Photo::id)
            .collect();
        for id in relative_ids {
            if let Some(photo) = project.photo_mut(id) {
                photo.set_path(base.join(photo.path.as_ref()));
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
        let missing = project.adjustments.load_luts();
        if missing > 0 {
            tracing::warn!(missing, "colour lookup tables could not be loaded");
        }
        Ok(project)
    }

    #[tracing::instrument(skip_all, fields(photos = self.photos.len()))]
    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let base = path.parent().unwrap_or_else(|| Path::new("."));
        let mut portable = self.clone();
        for photo in &self.photos {
            if let Ok(relative) = photo.path.strip_prefix(base)
                && let Some(target) = portable.photo_mut(photo.id())
            {
                target.set_path(relative.to_path_buf());
            }
        }
        let file = std::fs::File::create(path)?;
        let mut writer = BufWriter::new(file);
        serde_json::to_writer_pretty(&mut writer, &portable)?;
        writer.write_all(b"\n")?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Filter {
    All,
    Untagged,
    Tag(String),
    Unrated,
    Rated,
    Label(ColorLabel),
    Unlabeled,
}

impl Filter {
    #[must_use]
    pub fn matches(&self, photo: &Photo) -> bool {
        match self {
            Self::All => true,
            Self::Untagged => photo.tags.is_empty(),
            Self::Tag(tag) => photo.has_tag(tag),
            Self::Unrated => !photo.rating.is_rated(),
            Self::Rated => photo.rating.is_rated(),
            Self::Label(label) => photo.label == *label,
            Self::Unlabeled => !photo.label.is_set(),
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
    fn rating_and_label_roundtrip() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("rated.slate");
        let mut project = Project::new("Test");
        let id = project.add_photo(PathBuf::from("/tmp/a.jpg"), 100, 100, 1);
        {
            let photo = project.photo_mut(id).unwrap();
            photo.set_rating(StarRating::try_from(4).unwrap());
            photo.set_label(ColorLabel::Green);
        }
        project.save(&path).unwrap();
        let loaded = Project::load(&path).unwrap();
        let photo = loaded.photo(id).unwrap();
        assert_eq!(photo.rating(), StarRating::try_from(4).unwrap());
        assert_eq!(photo.label(), ColorLabel::Green);
    }

    #[test]
    fn older_photos_without_rating_or_label_open_unrated_and_unlabeled() {
        let mut project = Project::new("Test");
        let id = project.add_photo(PathBuf::from("/tmp/a.jpg"), 100, 100, 1);
        let mut value = serde_json::to_value(&project).unwrap();
        let photos = value
            .get_mut("photos")
            .and_then(|value| value.as_array_mut())
            .unwrap();
        let photo = photos[0].as_object_mut().unwrap();
        photo.remove("rating");
        photo.remove("label");
        let loaded: Project = serde_json::from_value(value).unwrap();
        let photo = loaded.photo(id).unwrap();
        assert_eq!(photo.rating(), StarRating::NONE);
        assert_eq!(photo.label(), ColorLabel::None);
    }

    #[test]
    fn photo_stays_compact() {
        assert!(
            std::mem::size_of::<Photo>() <= 80,
            "Photo grew to {} bytes",
            std::mem::size_of::<Photo>()
        );
    }

    #[test]
    fn persistent_photo_set_keeps_old_snapshots_alive() {
        let mut project = project_with_photos();
        let snapshot = project.clone();
        let id = PhotoId(1);
        project.photo_mut(id).unwrap().push_tag("new".into());
        assert!(snapshot.photo(id).unwrap().tags.is_empty());
        assert_eq!(project.photo(id).unwrap().tags.as_ref(), ["new"]);
        assert_eq!(project.photos.len(), snapshot.photos.len());
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
            .push_tag("new".into());
        assert!(!Arc::ptr_eq(&project.photos.data, &clone.photos.data));
        assert!(clone.photo(PhotoId(1)).unwrap().tags.is_empty());
    }

    #[test]
    fn chunk_offsets_stay_correct_after_removal() {
        let mut project = Project::new("Chunks");
        for index in 0..2_500u64 {
            project.add_photo(PathBuf::from(format!("/tmp/{index:04}.jpg")), 10, 10, 1);
        }
        project.remove_photos(&[PhotoId(500)]);
        assert_eq!(project.photos.len(), 2_499);
        assert_eq!(
            project.photos.get(499).map(Photo::id),
            Some(PhotoId(500 + 1))
        );
        assert_eq!(
            project.photos.get(2_498).map(Photo::id),
            Some(PhotoId(2_500))
        );
        assert_eq!(project.photo(PhotoId(2_500)).unwrap().id(), PhotoId(2_500));
        assert!(project.photo_mut(PhotoId(2_000)).is_some());
        project.add_photo(PathBuf::from("/tmp/last.jpg"), 10, 10, 1);
        assert_eq!(
            project.photos.get(2_499).map(Photo::id),
            Some(PhotoId(2_501))
        );
    }

    #[test]
    fn sparse_photo_ids_use_a_sparse_index() {
        let path = PathBuf::from("/tmp/sparse.jpg");
        let photo = Photo::new(PhotoId(10_000_000), path, PhotoSize::new(10, 10), 1);
        let mut set = PhotoSet::from_vec(vec![photo]).unwrap();
        assert_eq!(
            set.photo(PhotoId(10_000_000)).unwrap().id(),
            PhotoId(10_000_000)
        );
        set.push(Photo::new(
            PhotoId(10_000_001),
            PathBuf::from("/tmp/sparse-2.jpg"),
            PhotoSize::new(10, 10),
            1,
        ))
        .unwrap();
        assert_eq!(set.len(), 2);
        assert_eq!(set.position(PhotoId(10_000_001)), Some(1));
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
            .push_tag("street".into());
        project
            .photo_mut(PhotoId(2))
            .unwrap()
            .push_tag("street".into());
        project.photo_mut(PhotoId(1)).unwrap().rotation = QuarterTurns::try_from(1).unwrap();
        project.photo_mut(PhotoId(1)).unwrap().captured_at = Some("2025-08-17 09:41:03".into());
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
        assert_eq!(
            loaded.photos[0].captured_at.as_deref(),
            Some("2025-08-17 09:41:03")
        );
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
        project
            .photo_mut(PhotoId(1))
            .unwrap()
            .set_tags(vec!["zebra".into(), "alpha".into()]);
        project
            .photo_mut(PhotoId(2))
            .unwrap()
            .set_tags(vec!["alpha".into()]);
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
            .push_tag("street".into());
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
    fn adjustments_roundtrip_and_follow_their_photos() {
        use crate::adjust::Adjustments;

        let mut project = project_with_photos();
        let warm = Adjustments {
            temperature: 30.0,
            monochrome: true,
            ..Adjustments::default()
        };
        project.adjustments.set(PhotoId(1), warm);
        project.adjustments.set(PhotoId(2), warm);

        let json = serde_json::to_string(&project).unwrap();
        let mut back: Project = serde_json::from_str(&json).unwrap();
        assert_eq!(back.adjustments.for_photo(PhotoId(1)), warm);

        back.remove_photos(&[PhotoId(2)]);
        assert!(back.adjustments.get(PhotoId(2)).is_none());

        back.adjustments.set(PhotoId(99), warm);
        back.normalize();
        assert!(back.adjustments.get(PhotoId(99)).is_none());
        assert_eq!(back.adjustments.len(), 1);

        // Unadjusted collections keep writing the same file as before.
        let plain = serde_json::to_string(&project_with_photos()).unwrap();
        assert!(!plain.contains("adjustments"));
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
        assert!(project.adjustments.is_empty());
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
    fn saving_makes_in_folder_paths_relative() {
        let directory = tempfile::tempdir().unwrap();
        let folder = directory.path().join("Shoot");
        std::fs::create_dir_all(&folder).unwrap();
        let photo = folder.join("one.jpg");
        std::fs::write(&photo, "x").unwrap();
        let collection_path = folder.join("collection.slate");
        let mut project = Project::new("Shoot");
        project.add_photo(photo.clone(), 10, 10, 1);
        project.save(&collection_path).unwrap();

        let json = std::fs::read_to_string(&collection_path).unwrap();
        assert!(json.contains("\"one.jpg\""));
        assert!(!json.contains(photo.to_string_lossy().as_ref()));
        let loaded = Project::load(&collection_path).unwrap();
        assert_eq!(loaded.photos[0].path.as_ref(), photo.as_path());
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
            project.photos[0].path.as_ref(),
            directory.path().join("photos/a.jpg").as_path()
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
