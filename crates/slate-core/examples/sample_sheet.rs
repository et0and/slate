//! Generates a synthetic 12-photo project and exports it, for design checks.
//!
//! `cargo run -p slate-core --example sample_sheet`

use std::path::PathBuf;

use image::{ImageBuffer, Rgb};
use slate_core::{Project, batch, export_pdf};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/sample");
    std::fs::create_dir_all(&root)?;

    let mut project = Project::new("Street / Walk 26");
    project.sheet.title = "Street / Walk 26".into();
    project.sheet.columns = 3;
    project.sheet.rows = 4;
    project.sheet.rotate = false;

    let palettes = [
        ((28, 31, 34), (208, 198, 176)),
        ((178, 86, 48), (232, 216, 192)),
        ((54, 66, 78), (156, 172, 182)),
        ((34, 42, 36), (188, 202, 176)),
        ((120, 36, 28), (226, 172, 148)),
        ((72, 64, 88), (196, 190, 208)),
    ];

    for index in 0..12u32 {
        let (width, height) = match index % 4 {
            0 => (1600, 2400),
            1 => (2400, 1600),
            _ => (2000, 2000),
        };
        let (dark, light) = palettes[index as usize % palettes.len()];
        let image: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_fn(width, height, |x, y| {
            let fx = x as f32 / width as f32;
            let fy = y as f32 / height as f32;
            let mix = (fx * 0.55 + fy * 0.45).clamp(0.0, 1.0);
            let band = ((x + y) / 80) % 2;
            let mut color = Rgb([
                (dark.0 as f32 * (1.0 - mix) + light.0 as f32 * mix) as u8,
                (dark.1 as f32 * (1.0 - mix) + light.1 as f32 * mix) as u8,
                (dark.2 as f32 * (1.0 - mix) + light.2 as f32 * mix) as u8,
            ]);
            if band == 0 && y > height * 3 / 4 {
                color = Rgb([
                    color[0].saturating_sub(26),
                    color[1].saturating_sub(24),
                    color[2].saturating_sub(20),
                ]);
            }
            color
        });
        let name = format!("street-walk-{:02}.jpg", index + 1);
        let path = root.join(&name);
        image.save(&path)?;
        let id = project.add_photo(path, width, height, 4_000_000);
        let tags: &[&str] = match index % 3 {
            0 => &["street", "night"],
            1 => &["street", "portrait"],
            _ => &["walk", "film"],
        };
        if let Some(photo) = project.photo_mut(id) {
            photo.tags = tags.iter().map(|tag| tag.to_string()).collect();
        }
    }

    let ids = project.photo_ids();
    let _ = batch::add_tag(&mut project, &ids[0..3], "select");

    let project_path = root.join("sample.contactsheet");
    project.save(&project_path)?;
    let pdf_path = root.join("sample.pdf");
    let summary = export_pdf(&project.name, &project.sheet, &project.photos, &pdf_path)?;
    println!("project: {}", project_path.display());
    println!(
        "pdf: {} ({} pages, {} photos, {} bytes)",
        pdf_path.display(),
        summary.pages,
        summary.photos,
        summary.bytes
    );

    let mut rotated = project.sheet.clone();
    rotated.orientation = slate_core::Orientation::Landscape;
    rotated.columns = 4;
    rotated.rows = 3;
    rotated.rotate = true;
    rotated.title = "Rotated / fit".into();
    export_pdf(
        &project.name,
        &rotated,
        &project.photos,
        &root.join("sample-rotated.pdf"),
    )?;

    let mut clean = project.sheet.clone();
    clean.captions = false;
    clean.index = true;
    clean.header = false;
    clean.title = "Index only".into();
    export_pdf(
        &project.name,
        &clean,
        &project.photos,
        &root.join("sample-index.pdf"),
    )?;
    Ok(())
}
