//! Contact sheet geometry.
//!
//! All positions are top-left based millimetres so the same plan can drive the
//! on-screen preview and the PDF. PDF drawing converts to bottom-left points in
//! `pdf.rs`.

use crate::model::{Photo, PhotoSet, SheetSettings};

pub const MM_TO_PT: f32 = 72.0 / 25.4;

const HEADER_MM: f32 = 16.0;
const FOOTER_MM: f32 = 10.0;
const CAPTION_MM: f32 = 6.0;

#[derive(Debug, Clone, Copy)]
pub struct SheetLayout {
    pub page_w_mm: f32,
    pub page_h_mm: f32,
    pub margin_mm: f32,
    pub header_mm: f32,
    pub footer_mm: f32,
    pub content_top_mm: f32,
    pub content_bottom_mm: f32,
    pub cell_w_mm: f32,
    pub cell_h_mm: f32,
    pub image_w_mm: f32,
    pub image_h_mm: f32,
    pub caption_mm: f32,
    pub columns: usize,
    pub rows: usize,
}

impl SheetLayout {
    pub fn from_settings(settings: &SheetSettings) -> Self {
        let (page_w_mm, page_h_mm) = settings.paper_mm();
        let margin_mm = settings.margin_mm.clamp(2.0, 40.0);
        let gutter_mm = settings.gutter_mm.clamp(0.0, 20.0);
        let columns = usize::from(settings.columns.clamp(1, 12));
        let rows = usize::from(settings.rows.clamp(1, 12));

        let header_mm = if settings.header { HEADER_MM } else { 0.0 };
        let footer_mm = if settings.footer { FOOTER_MM } else { 0.0 };
        let content_top_mm = margin_mm + header_mm;
        let content_bottom_mm = (page_h_mm - margin_mm - footer_mm).max(content_top_mm);
        let content_h_mm = content_bottom_mm - content_top_mm;
        let content_w_mm = (page_w_mm - margin_mm * 2.0).max(1.0);

        let cell_w_mm = ((content_w_mm - gutter_mm * (columns.saturating_sub(1) as f32))
            / columns as f32)
            .max(1.0);
        let cell_h_mm =
            ((content_h_mm - gutter_mm * (rows.saturating_sub(1) as f32)) / rows as f32).max(1.0);

        let wants_caption = settings.captions || settings.index;
        let caption_mm = if wants_caption {
            CAPTION_MM.min(cell_h_mm * 0.4)
        } else {
            0.0
        };

        Self {
            page_w_mm,
            page_h_mm,
            margin_mm,
            header_mm,
            footer_mm,
            content_top_mm,
            content_bottom_mm,
            cell_w_mm,
            cell_h_mm,
            image_w_mm: cell_w_mm,
            image_h_mm: (cell_h_mm - caption_mm).max(1.0),
            caption_mm,
            columns,
            rows,
        }
    }

    pub fn per_page(&self) -> usize {
        self.columns * self.rows
    }

    pub fn caption_enabled(&self) -> bool {
        self.caption_mm > 0.0
    }
}

#[derive(Debug, Clone)]
pub struct Placement {
    pub photo_index: usize,
    pub global_index: usize,
    pub slot: usize,
    pub cell_x_mm: f32,
    pub cell_y_mm: f32,
    pub image_x_mm: f32,
    pub image_y_mm: f32,
    pub image_w_mm: f32,
    pub image_h_mm: f32,
    pub rotated: bool,
    pub caption: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PagePlan {
    pub number: usize,
    pub placements: Vec<Placement>,
}

#[derive(Debug, Clone)]
pub struct SheetPlan {
    pub layout: SheetLayout,
    pub pages: Vec<PagePlan>,
    pub photo_count: usize,
}

impl SheetPlan {
    pub fn page_count(&self) -> usize {
        self.pages.len()
    }

    pub fn is_empty(&self) -> bool {
        self.photo_count == 0
    }
}

/// Fit `image_ar` inside `area_ar` and return the resulting width, height and
/// whether rotating the image by 90° produces a better fill. Rotation is only
/// considered when the result is actually larger -- a 3:2 frame in a square
/// cell should stay upright even if the cell is technically landscape.
fn fit(image_ar: f32, area_w: f32, area_h: f32, allow_rotation: bool) -> (f32, f32, bool) {
    let image_ar = image_ar.max(0.0001);
    let area_ar = area_w / area_h.max(0.0001);

    let fit_for = |ar: f32| {
        if ar >= area_ar {
            (area_w, area_w / ar)
        } else {
            (area_h * ar, area_h)
        }
    };

    let (width, height) = fit_for(image_ar);
    if !allow_rotation {
        return (width, height, false);
    }

    let (rotated_w, rotated_h) = fit_for(1.0 / image_ar);
    if rotated_w * rotated_h > width * height * 1.0001 {
        (rotated_w, rotated_h, true)
    } else {
        (width, height, false)
    }
}

/// Random-access photo storage used by whole-sheet planning.
///
/// `PhotoSet` is persistent; tests and small callers can pass a slice or
/// `Vec<Photo>` without building a `PhotoSet`.
pub trait PhotoSource {
    fn source_len(&self) -> usize;
    fn source_get(&self, index: usize) -> Option<&Photo>;
}

impl PhotoSource for [Photo] {
    fn source_len(&self) -> usize {
        self.len()
    }

    fn source_get(&self, index: usize) -> Option<&Photo> {
        self.get(index)
    }
}

impl PhotoSource for Vec<Photo> {
    fn source_len(&self) -> usize {
        self.len()
    }

    fn source_get(&self, index: usize) -> Option<&Photo> {
        self.get(index)
    }
}

impl<const N: usize> PhotoSource for [Photo; N] {
    fn source_len(&self) -> usize {
        N
    }

    fn source_get(&self, index: usize) -> Option<&Photo> {
        self.get(index)
    }
}

impl PhotoSource for PhotoSet {
    fn source_len(&self) -> usize {
        self.len()
    }

    fn source_get(&self, index: usize) -> Option<&Photo> {
        self.get(index)
    }
}

pub fn plan_sheet<P: PhotoSource + ?Sized>(settings: &SheetSettings, photos: &P) -> SheetPlan {
    let layout = SheetLayout::from_settings(settings);
    let per_page = layout.per_page().max(1);
    let page_count = photos.source_len().div_ceil(per_page).max(1);
    let pages = (0..page_count)
        .map(|page_index| plan_page(settings, &layout, photos, page_index))
        .collect();

    SheetPlan {
        layout,
        pages,
        photo_count: photos.source_len(),
    }
}

/// Lay out one page without touching the rest of a large library. The preview
/// uses this so a 10k-photo project still renders quickly.
fn placement(
    settings: &SheetSettings,
    layout: &SheetLayout,
    photo: &Photo,
    photo_index: usize,
    global_index: usize,
    slot: usize,
) -> Placement {
    let row = slot / layout.columns;
    let column = slot % layout.columns;
    let cell_x_mm = layout.margin_mm + column as f32 * (layout.cell_w_mm + settings.gutter_mm);
    let cell_y_mm = layout.content_top_mm + row as f32 * (layout.cell_h_mm + settings.gutter_mm);

    let (image_w_mm, image_h_mm, rotated) = fit(
        photo.aspect().unwrap_or(1.0),
        layout.image_w_mm,
        layout.image_h_mm,
        settings.rotate,
    );
    let image_x_mm = cell_x_mm + (layout.image_w_mm - image_w_mm) / 2.0;
    let image_y_mm = cell_y_mm + (layout.image_h_mm - image_h_mm) / 2.0;

    Placement {
        photo_index,
        global_index: global_index + 1,
        slot,
        cell_x_mm,
        cell_y_mm,
        image_x_mm,
        image_y_mm,
        image_w_mm,
        image_h_mm,
        rotated,
        caption: caption_text(settings, global_index, photo),
    }
}

pub fn plan_page<P: PhotoSource + ?Sized>(
    settings: &SheetSettings,
    layout: &SheetLayout,
    photos: &P,
    page_index: usize,
) -> PagePlan {
    let per_page = layout.per_page().max(1);
    let start = page_index.saturating_mul(per_page);
    let end = (start + per_page).min(photos.source_len());
    let mut placements = Vec::with_capacity(end.saturating_sub(start));

    for (slot, photo_index) in (start..end).enumerate() {
        let Some(photo) = photos.source_get(photo_index) else {
            continue;
        };
        placements.push(placement(
            settings,
            layout,
            photo,
            photo_index,
            photo_index,
            slot,
        ));
    }

    PagePlan {
        number: page_index + 1,
        placements,
    }
}

/// Lay out one page from borrowed photos. The preview uses this so it does not clone
/// the whole working set just to render twelve cells.
pub fn plan_page_refs(
    settings: &SheetSettings,
    layout: &SheetLayout,
    photos: &[&Photo],
    first_index: usize,
    page_index: usize,
) -> PagePlan {
    let mut placements = Vec::with_capacity(photos.len());
    for (slot, photo) in photos.iter().enumerate() {
        placements.push(placement(
            settings,
            layout,
            photo,
            slot,
            first_index + slot,
            slot,
        ));
    }
    PagePlan {
        number: page_index + 1,
        placements,
    }
}

pub fn caption_text(settings: &SheetSettings, photo_index: usize, photo: &Photo) -> Option<String> {
    if !settings.captions && !settings.index {
        return None;
    }
    let width = if photo_index + 1 >= 1000 { 4 } else { 3 };
    let index = format!("{:0width$}", photo_index + 1);
    Some(match (settings.index, settings.captions) {
        (true, true) => format!("{index}  {}", photo.file_name()),
        (true, false) => index,
        (false, true) => photo.file_name(),
        (false, false) => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Orientation, Paper, PhotoId, PhotoSize};
    use std::path::PathBuf;

    fn photo(width: u32, height: u32) -> Photo {
        Photo::new(
            PhotoId(1),
            PathBuf::from("/tmp/image.jpg"),
            PhotoSize::new(width, height),
            0,
        )
    }

    #[test]
    fn a4_portrait_grid_fills_the_page() {
        let settings = SheetSettings::default();
        let layout = SheetLayout::from_settings(&settings);

        assert_eq!(layout.columns, 3);
        assert_eq!(layout.rows, 4);
        assert_eq!(layout.per_page(), 12);
        assert!(layout.cell_w_mm > 55.0 && layout.cell_w_mm < 65.0);
        assert!(layout.cell_h_mm > 55.0 && layout.cell_h_mm < 65.0);
        assert!(layout.caption_enabled());
        assert!(layout.image_h_mm < layout.cell_h_mm);
        assert!(layout.content_bottom_mm < layout.page_h_mm - layout.margin_mm);
    }

    #[test]
    fn custom_paper_uses_free_entry_dimensions() {
        let settings = SheetSettings {
            paper: Paper::Custom,
            custom_paper_width_mm: 120.0,
            custom_paper_height_mm: 180.0,
            ..Default::default()
        };
        let layout = SheetLayout::from_settings(&settings);
        assert!((layout.page_w_mm - 120.0).abs() < 0.01);
        assert!((layout.page_h_mm - 180.0).abs() < 0.01);
    }

    #[test]
    fn custom_paper_respects_orientation() {
        let settings = SheetSettings {
            paper: Paper::Custom,
            custom_paper_width_mm: 120.0,
            custom_paper_height_mm: 180.0,
            orientation: Orientation::Landscape,
            ..Default::default()
        };
        let (width, height) = settings.paper_mm();
        assert!((width - 180.0).abs() < 0.01);
        assert!((height - 120.0).abs() < 0.01);
    }

    #[test]
    fn landscape_swap_is_exact() {
        let settings = SheetSettings {
            paper: Paper::Letter,
            orientation: Orientation::Landscape,
            ..Default::default()
        };
        let (width, height) = settings.paper_mm();
        assert!(width > height);
        assert!((width - 279.4).abs() < 0.01);
        assert!((height - 215.9).abs() < 0.01);
    }

    #[test]
    fn pages_split_on_capacity() {
        let settings = SheetSettings {
            columns: 2,
            rows: 2,
            ..Default::default()
        };
        let photos: Vec<Photo> = (0..5).map(|_| photo(100, 100)).collect();
        let plan = plan_sheet(&settings, &photos);
        assert_eq!(plan.page_count(), 2);
        assert_eq!(plan.pages[0].placements.len(), 4);
        assert_eq!(plan.pages[1].placements.len(), 1);
        assert_eq!(plan.pages[1].placements[0].global_index, 5);
    }

    #[test]
    fn plan_page_matches_full_plan() {
        let settings = SheetSettings::default();
        let photos: Vec<Photo> = (0..17).map(|_| photo(3000, 2000)).collect();
        let plan = plan_sheet(&settings, &photos);
        let layout = SheetLayout::from_settings(&settings);
        for (index, page) in plan.pages.iter().enumerate() {
            let single = plan_page(&settings, &layout, &photos, index);
            assert_eq!(page.number, single.number);
            assert_eq!(page.placements.len(), single.placements.len());
            assert_eq!(
                page.placements
                    .first()
                    .map(|placement| placement.photo_index),
                single
                    .placements
                    .first()
                    .map(|placement| placement.photo_index)
            );
        }
    }

    #[test]
    fn borrowed_page_plan_matches_the_full_plan() {
        let settings = SheetSettings::default();
        let photos: Vec<Photo> = (0..30).map(|_| photo(3000, 2000)).collect();
        let full = plan_page(
            &settings,
            &SheetLayout::from_settings(&settings),
            &photos,
            1,
        );
        let refs: Vec<&Photo> = photos[12..24].iter().collect();
        let borrowed = plan_page_refs(
            &settings,
            &SheetLayout::from_settings(&settings),
            &refs,
            12,
            1,
        );
        assert_eq!(borrowed.number, full.number);
        assert_eq!(borrowed.placements.len(), full.placements.len());
        assert_eq!(
            borrowed.placements[0].global_index,
            full.placements[0].global_index
        );
        assert_eq!(borrowed.placements[0].caption, full.placements[0].caption);
    }

    #[test]
    fn empty_plan_still_has_one_page() {
        let plan = plan_sheet(&SheetSettings::default(), &[]);
        assert_eq!(plan.page_count(), 1);
        assert!(plan.pages[0].placements.is_empty());
    }

    #[test]
    fn rotation_only_when_it_fills_better() {
        let settings = SheetSettings {
            columns: 1,
            rows: 1,
            header: false,
            footer: false,
            captions: false,
            index: false,
            rotate: true,
            ..Default::default()
        };

        let portrait_page = plan_sheet(&settings, &[photo(3000, 2000)]);
        assert!(portrait_page.pages[0].placements[0].rotated);
        assert!(portrait_page.pages[0].placements[0].image_w_mm < 200.0);

        let mut landscape = settings.clone();
        landscape.orientation = Orientation::Landscape;
        let landscape_page = plan_sheet(&landscape, &[photo(3000, 2000)]);
        assert!(!landscape_page.pages[0].placements[0].rotated);
    }

    #[test]
    fn rotation_can_be_disabled() {
        let settings = SheetSettings {
            rotate: false,
            ..Default::default()
        };
        let plan = plan_sheet(&settings, &[photo(3000, 2000)]);
        assert!(!plan.pages[0].placements[0].rotated);
    }

    #[test]
    fn captions_follow_settings() {
        let photo = photo(100, 100);
        let mut settings = SheetSettings::default();
        assert_eq!(
            caption_text(&settings, 0, &photo).as_deref(),
            Some("001  image.jpg")
        );
        settings.captions = false;
        assert_eq!(caption_text(&settings, 0, &photo).as_deref(), Some("001"));
        settings.index = false;
        assert_eq!(caption_text(&settings, 0, &photo), None);
    }

    #[test]
    fn extreme_settings_do_not_produce_zero_sizes() {
        let settings = SheetSettings {
            columns: 12,
            rows: 12,
            margin_mm: 40.0,
            gutter_mm: 20.0,
            ..Default::default()
        };
        let layout = SheetLayout::from_settings(&settings);
        assert!(layout.cell_w_mm > 0.0);
        assert!(layout.cell_h_mm > 0.0);
        assert!(layout.image_h_mm > 0.0);
    }
}
