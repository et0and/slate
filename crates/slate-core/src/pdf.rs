//! Flat PDF export.
//!
//! The same [`plan_sheet`] output that drives the in-app preview drives the PDF.
//! Photos are embedded at up to 300 DPI for their cell, so contact sheets stay
//! printable without carrying full-resolution originals into the file.

use std::path::{Path, PathBuf};

use anyhow::{Context as _, Result};
use printpdf::*;

use crate::import::load_oriented;
use crate::layout::{MM_TO_PT, Placement, SheetLayout, plan_sheet};
use crate::model::{Photo, PhotoId, SheetSettings};

const DPI: f32 = 300.0;
const TEXT_BLACK: (f32, f32, f32) = (0.04, 0.04, 0.04);
const TEXT_GREY: (f32, f32, f32) = (0.42, 0.42, 0.42);
const HAIRLINE: (f32, f32, f32) = (0.78, 0.77, 0.74);

#[derive(Debug, Clone)]
pub struct ExportSummary {
    pub path: PathBuf,
    pub pages: usize,
    pub photos: usize,
    pub missing: Vec<String>,
    pub bytes: usize,
    pub warnings: usize,
}

fn mm_to_pt(mm: f32) -> f32 {
    mm * MM_TO_PT
}

fn top_to_bottom(page_height_mm: f32, top_mm: f32) -> f32 {
    page_height_mm - top_mm
}

fn color(rgb: (f32, f32, f32)) -> Color {
    Color::Rgb(Rgb {
        r: rgb.0,
        g: rgb.1,
        b: rgb.2,
        icc_profile: None,
    })
}

fn text_op(
    page_h_mm: f32,
    x_mm: f32,
    top_mm: f32,
    size: f32,
    font: BuiltinFont,
    fill: (f32, f32, f32),
    value: &str,
) -> Vec<Op> {
    vec![
        Op::StartTextSection,
        Op::SetTextCursor {
            pos: Point::new(Mm(x_mm), Mm(top_to_bottom(page_h_mm, top_mm))),
        },
        Op::SetFont {
            font: PdfFontHandle::Builtin(font),
            size: Pt(size),
        },
        Op::SetFillColor { col: color(fill) },
        Op::ShowText {
            items: vec![TextItem::Text(value.to_string())],
        },
        Op::EndTextSection,
    ]
}

fn hairline(
    x1_mm: f32,
    y1_mm: f32,
    x2_mm: f32,
    y2_mm: f32,
    thickness: f32,
    fill: (f32, f32, f32),
) -> Vec<Op> {
    vec![
        Op::SetOutlineColor { col: color(fill) },
        Op::SetOutlineThickness { pt: Pt(thickness) },
        Op::DrawLine {
            line: Line {
                points: vec![
                    LinePoint {
                        p: Point::new(Mm(x1_mm), Mm(y1_mm)),
                        bezier: false,
                    },
                    LinePoint {
                        p: Point::new(Mm(x2_mm), Mm(y2_mm)),
                        bezier: false,
                    },
                ],
                is_closed: false,
            },
        },
    ]
}

fn rectangle(x_mm: f32, top_mm: f32, width_mm: f32, height_mm: f32, page_h_mm: f32) -> Vec<Op> {
    let bottom_mm = top_to_bottom(page_h_mm, top_mm + height_mm);
    vec![
        Op::SetOutlineColor {
            col: color(HAIRLINE),
        },
        Op::SetOutlineThickness { pt: Pt(0.4) },
        Op::DrawRectangle {
            rectangle: Rect {
                x: Pt(mm_to_pt(x_mm)),
                y: Pt(mm_to_pt(bottom_mm)),
                width: Pt(mm_to_pt(width_mm)),
                height: Pt(mm_to_pt(height_mm)),
                mode: Some(PaintMode::Stroke),
                winding_order: None,
            },
        },
    ]
}

fn flatten_on_white(image: ::image::DynamicImage) -> ::image::RgbImage {
    let mut rgba = image.to_rgba8();
    for pixel in rgba.as_chunks_mut::<4>().0 {
        let alpha = u16::from(pixel[3]);
        if alpha < 255 {
            let blend =
                |channel: u8| ((u16::from(channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
            pixel[0] = blend(pixel[0]);
            pixel[1] = blend(pixel[1]);
            pixel[2] = blend(pixel[2]);
        }
        pixel[3] = 255;
    }
    let mut rgb = ::image::RgbImage::new(rgba.width(), rgba.height());
    for (source, target) in rgba.pixels().zip(rgb.pixels_mut()) {
        target[0] = source[0];
        target[1] = source[1];
        target[2] = source[2];
    }
    rgb
}

fn ascii_lossy(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii() && !character.is_control() {
                character
            } else {
                '?'
            }
        })
        .collect()
}

fn truncate_chars(value: &str, max_chars: usize) -> String {
    if value.chars().count() <= max_chars || max_chars < 2 {
        return value.to_string();
    }
    let keep = max_chars.saturating_sub(1);
    let mut truncated: String = value.chars().take(keep).collect();
    truncated.push('…');
    truncated
}

fn truncate_courier(value: &str, width_mm: f32, size_pt: f32) -> String {
    let max_chars = ((width_mm * MM_TO_PT) / (size_pt * 0.6)).floor() as usize;
    truncate_chars(&ascii_lossy(value), max_chars)
}

fn draw_header(
    ops: &mut Vec<Op>,
    layout: &SheetLayout,
    settings: &SheetSettings,
    project_name: &str,
    page: usize,
    page_count: usize,
    photo_count: usize,
) {
    let title = ascii_lossy(settings.title_or(project_name));
    let title_width = layout.page_w_mm - layout.margin_mm * 2.0;
    let title = truncate_chars(&title, (title_width * MM_TO_PT / (13.0 * 0.58)) as usize);
    ops.extend(text_op(
        layout.page_h_mm,
        layout.margin_mm,
        layout.margin_mm + 6.0,
        13.0,
        BuiltinFont::HelveticaBold,
        TEXT_BLACK,
        &title,
    ));

    let meta = format!(
        "Contact sheet   {} photos   {} {}   {} x {}",
        photo_count,
        settings.paper.label(),
        settings.orientation.label(),
        settings.columns,
        settings.rows
    );
    ops.extend(text_op(
        layout.page_h_mm,
        layout.margin_mm,
        layout.margin_mm + 10.5,
        6.0,
        BuiltinFont::Courier,
        TEXT_GREY,
        &meta,
    ));

    let page_label = format!("Sheet {:02} / {:02}", page, page_count);
    let field_width = layout.page_w_mm - layout.margin_mm * 2.0;
    let x = layout.margin_mm + field_width - page_label.chars().count() as f32 * 1.28;
    ops.extend(text_op(
        layout.page_h_mm,
        x,
        layout.margin_mm + 10.5,
        6.0,
        BuiltinFont::Courier,
        TEXT_GREY,
        &page_label,
    ));

    let rule_y = top_to_bottom(layout.page_h_mm, layout.content_top_mm - 3.0);
    ops.extend(hairline(
        layout.margin_mm,
        rule_y,
        layout.page_w_mm - layout.margin_mm,
        rule_y,
        0.4,
        HAIRLINE,
    ));
}

fn draw_footer(
    ops: &mut Vec<Op>,
    layout: &SheetLayout,
    project_name: &str,
    page: usize,
    page_count: usize,
) {
    let rule_top = layout.page_h_mm - layout.margin_mm - 7.0;
    let rule_y = top_to_bottom(layout.page_h_mm, rule_top);
    ops.extend(hairline(
        layout.margin_mm,
        rule_y,
        layout.page_w_mm - layout.margin_mm,
        rule_y,
        0.4,
        HAIRLINE,
    ));

    let label = ascii_lossy(project_name);
    let label_width = layout.page_w_mm - layout.margin_mm * 2.0 - 40.0;
    let label = truncate_courier(&label, label_width, 6.0);
    ops.extend(text_op(
        layout.page_h_mm,
        layout.margin_mm,
        rule_top + 4.0,
        6.0,
        BuiltinFont::Courier,
        TEXT_GREY,
        &label,
    ));

    let page_label = format!("Page {:02} / {:02}", page, page_count);
    let x = layout.margin_mm + (layout.page_w_mm - layout.margin_mm * 2.0)
        - page_label.chars().count() as f32 * 1.28;
    ops.extend(text_op(
        layout.page_h_mm,
        x,
        rule_top + 4.0,
        6.0,
        BuiltinFont::Courier,
        TEXT_GREY,
        &page_label,
    ));
}

enum ImageOutcome {
    Placed,
    Missing(String),
}

fn draw_photo(
    doc: &mut PdfDocument,
    ops: &mut Vec<Op>,
    layout: &SheetLayout,
    placement: &Placement,
    photo: &Photo,
) -> ImageOutcome {
    let image_area_w = placement.image_w_mm;
    let image_area_h = placement.image_h_mm;

    // Cap decoding to what 300 DPI can show for this cell. The extra headroom
    // avoids softening after 90 degree rotation.
    let max_px = (((image_area_w.max(image_area_h) * DPI) / 25.4).ceil() as u32)
        .max(64)
        .saturating_mul(2);

    let image = match load_oriented(&photo.path, Some(max_px)) {
        Ok(image) => image,
        Err(error) => {
            ops.extend(rectangle(
                placement.image_x_mm,
                placement.image_y_mm,
                image_area_w,
                image_area_h,
                layout.page_h_mm,
            ));
            ops.extend(text_op(
                layout.page_h_mm,
                placement.image_x_mm + 4.0,
                placement.image_y_mm + image_area_h / 2.0,
                6.0,
                BuiltinFont::Courier,
                TEXT_GREY,
                "Missing",
            ));
            return ImageOutcome::Missing(format!("{}: {error}", photo.file_name()));
        }
    };

    let mut image = image;
    for _ in 0..photo.rotation() {
        image = image.rotate90();
    }
    if placement.rotated {
        image = image.rotate90();
    }

    let target_w = ((placement.image_w_mm * DPI) / 25.4).ceil() as u32;
    let target_h = ((placement.image_h_mm * DPI) / 25.4).ceil() as u32;
    let image = flatten_on_white(image.thumbnail(target_w.max(1), target_h.max(1)));
    let (pixel_width, pixel_height) = (image.width(), image.height());
    let raw = match RawImage::from_dynamic_image(::image::DynamicImage::from(image)) {
        Ok(raw) => raw,
        Err(error) => {
            return ImageOutcome::Missing(format!("{}: {error}", photo.file_name()));
        }
    };
    let xobject = doc.add_image(&raw);

    let width_pt = mm_to_pt(placement.image_w_mm);
    let height_pt = mm_to_pt(placement.image_h_mm);
    let y_pt = mm_to_pt(top_to_bottom(
        layout.page_h_mm,
        placement.image_y_mm + placement.image_h_mm,
    ));

    ops.push(Op::SaveGraphicsState);
    ops.push(Op::UseXobject {
        id: xobject,
        transform: XObjectTransform {
            translate_x: Some(Pt(mm_to_pt(placement.image_x_mm))),
            translate_y: Some(Pt(y_pt)),
            scale_x: Some(width_pt / pixel_width.max(1) as f32),
            scale_y: Some(height_pt / pixel_height.max(1) as f32),
            dpi: Some(72.0),
            ..Default::default()
        },
    });
    ops.push(Op::RestoreGraphicsState);

    ops.extend(rectangle(
        placement.image_x_mm,
        placement.image_y_mm,
        image_area_w,
        image_area_h,
        layout.page_h_mm,
    ));
    ImageOutcome::Placed
}

fn draw_caption(ops: &mut Vec<Op>, layout: &SheetLayout, placement: &Placement, caption: &str) {
    let caption_top = placement.cell_y_mm + layout.image_h_mm;
    let rule_y = top_to_bottom(layout.page_h_mm, caption_top);
    ops.extend(hairline(
        placement.cell_x_mm,
        rule_y,
        placement.cell_x_mm + layout.cell_w_mm,
        rule_y,
        0.3,
        HAIRLINE,
    ));
    let value = truncate_courier(caption, layout.cell_w_mm, 6.0);
    ops.extend(text_op(
        layout.page_h_mm,
        placement.cell_x_mm + 0.5,
        caption_top + 3.6,
        6.0,
        BuiltinFont::Courier,
        TEXT_GREY,
        &value,
    ));
}

/// Render selected photos into a flat PDF at `output`.
pub fn export_pdf(
    project_name: &str,
    settings: &SheetSettings,
    photos: &[Photo],
    output: &Path,
) -> Result<ExportSummary> {
    let plan = plan_sheet(settings, photos);
    let mut document = PdfDocument::new(settings.title_or(project_name));
    let mut pages = Vec::with_capacity(plan.pages.len());
    let mut missing = Vec::new();
    let page_count = plan.pages.len();

    for page in &plan.pages {
        let mut ops = Vec::new();

        if settings.header {
            draw_header(
                &mut ops,
                &plan.layout,
                settings,
                project_name,
                page.number,
                page_count,
                photos.len(),
            );
        }

        if page.placements.is_empty() {
            ops.extend(text_op(
                plan.layout.page_h_mm,
                plan.layout.margin_mm,
                plan.layout.content_top_mm + 10.0,
                7.0,
                BuiltinFont::Courier,
                TEXT_GREY,
                "No photos selected",
            ));
        }

        for placement in &page.placements {
            let Some(photo) = photos.get(placement.photo_index) else {
                continue;
            };
            if let ImageOutcome::Missing(reason) =
                draw_photo(&mut document, &mut ops, &plan.layout, placement, photo)
            {
                missing.push(reason);
            }
            if let Some(caption) = &placement.caption {
                draw_caption(&mut ops, &plan.layout, placement, caption);
            }
        }

        if settings.footer {
            draw_footer(
                &mut ops,
                &plan.layout,
                project_name,
                page.number,
                page_count,
            );
        }

        pages.push(PdfPage::new(
            Mm(plan.layout.page_w_mm),
            Mm(plan.layout.page_h_mm),
            ops,
        ));
    }

    document.with_pages(pages);
    let mut warnings = Vec::new();
    let bytes = document.save(&PdfSaveOptions::default(), &mut warnings);
    if bytes.is_empty() {
        anyhow::bail!("PDF renderer returned no data");
    }
    if let Some(parent) = output.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("could not create {}", parent.display()))?;
    }
    std::fs::write(output, &bytes)
        .with_context(|| format!("could not write {}", output.display()))?;

    Ok(ExportSummary {
        path: output.to_path_buf(),
        pages: page_count,
        photos: photos.len(),
        missing,
        bytes: bytes.len(),
        warnings: warnings.len(),
    })
}

/// Export a saved sheet by resolving its stored photo ids.
pub fn export_saved_sheet(
    project: &crate::model::Project,
    sheet: &crate::model::SavedSheet,
    output: &Path,
) -> Result<ExportSummary> {
    let photos: Vec<Photo> = sheet
        .photo_ids
        .iter()
        .filter_map(|id| project.photo(*id).cloned())
        .collect();
    export_pdf(&project.name, &sheet.settings, &photos, output)
}

pub fn missing_ids(photos: &[Photo]) -> Vec<PhotoId> {
    photos
        .iter()
        .filter(|photo| !photo.path.exists())
        .map(|photo| photo.id)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Paper, PhotoId, Project};
    use ::image::{ImageBuffer, Rgb};

    fn image_path(directory: &Path, name: &str, width: u32, height: u32) -> PathBuf {
        let path = directory.join(name);
        let image: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 255) as u8, (y % 255) as u8, 120])
        });
        image.save(&path).unwrap();
        path
    }

    fn photo(id: u64, path: PathBuf, width: u32, height: u32) -> Photo {
        Photo {
            id: PhotoId(id),
            path,
            width,
            height,
            bytes: 1,
            tags: vec![],
            rotation: 0,
        }
    }

    #[test]
    fn exports_a_flat_pdf_with_embedded_photos() {
        let directory = tempfile::tempdir().unwrap();
        let first = image_path(directory.path(), "a.jpg", 600, 400);
        let second = image_path(directory.path(), "b.jpg", 400, 600);
        let output = directory.path().join("sheet.pdf");

        let settings = SheetSettings {
            columns: 2,
            rows: 1,
            paper: Paper::A4,
            ..Default::default()
        };
        let photos = vec![photo(1, first, 600, 400), photo(2, second, 400, 600)];

        let summary = export_pdf("Test", &settings, &photos, &output).unwrap();
        assert_eq!(summary.pages, 1);
        assert_eq!(summary.photos, 2);
        assert!(summary.missing.is_empty());
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 4_000, "pdf should contain image data");
        assert_eq!(summary.bytes, bytes.len());
    }

    #[test]
    fn saved_sheet_survives_a_project_roundtrip_and_exports() {
        let directory = tempfile::tempdir().unwrap();
        let first = image_path(directory.path(), "a.jpg", 600, 400);
        let second = image_path(directory.path(), "b.jpg", 400, 600);
        let mut project = Project::new("Saved Test");
        let first_id = project.add_photo(first, 600, 400, 1);
        project.add_photo(second, 400, 600, 1);
        project.sheet.columns = 1;
        project.sheet.rows = 1;
        project.sheet.title = "Keep Me".into();
        project.upsert_saved_sheet("Keep Me".into(), project.sheet.clone(), vec![first_id]);

        let project_path = directory.path().join("saved.contactsheet");
        project.save(&project_path).unwrap();
        let loaded = Project::load(&project_path).unwrap();
        let sheet = loaded.saved_sheets.first().unwrap().clone();
        let pdf_path = directory.path().join("saved.pdf");
        let summary = export_saved_sheet(&loaded, &sheet, &pdf_path).unwrap();

        assert_eq!(summary.photos, 1);
        assert_eq!(summary.pages, 1);
        assert!(summary.missing.is_empty());
        assert!(pdf_path.exists());
    }

    #[test]
    fn missing_photo_becomes_a_placeholder_not_a_failure() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("missing.pdf");
        let photos = vec![photo(1, directory.path().join("gone.jpg"), 100, 100)];
        let summary = export_pdf("Test", &SheetSettings::default(), &photos, &output).unwrap();
        assert_eq!(summary.missing.len(), 1);
        assert!(output.exists());
    }

    #[test]
    fn multipage_export_counts_pages() {
        let directory = tempfile::tempdir().unwrap();
        let path = image_path(directory.path(), "a.jpg", 100, 100);
        let settings = SheetSettings {
            columns: 1,
            rows: 1,
            captions: false,
            ..Default::default()
        };
        let photos: Vec<Photo> = (0..3)
            .map(|index| photo(index + 1, path.clone(), 100, 100))
            .collect();
        let output = directory.path().join("three.pdf");
        let summary = export_pdf("Test", &settings, &photos, &output).unwrap();
        assert_eq!(summary.pages, 3);
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.len() > 10_000);
    }

    #[test]
    fn ascii_lossy_keeps_ascii_and_replaces_the_rest() {
        assert_eq!(ascii_lossy("ABC-123.jpg"), "ABC-123.jpg");
        assert_eq!(ascii_lossy("café.jpg"), "caf?.jpg");
    }

    #[test]
    fn courier_truncation_respects_width() {
        let long = "A".repeat(200);
        let value = truncate_courier(&long, 50.0, 6.0);
        assert!(value.chars().count() < 50);
        assert!(value.ends_with('…'));
    }

    #[test]
    fn title_truncation_is_safe_for_short_limits() {
        assert_eq!(truncate_chars("ABCD", 10), "ABCD");
        assert_eq!(truncate_chars("ABCDEFGH", 4), "ABC…");
        assert_eq!(truncate_chars("ABCD", 1), "ABCD");
    }
}
