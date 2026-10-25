//! Generates a synthetic 12-photo project and exports it, for design checks.
//!
//! `cargo run -p slate-core --example sample_sheet`

use std::path::{Path, PathBuf};

use slate_core::{Project, batch, export_pdf};

fn main() -> anyhow::Result<()> {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/sample");
    std::fs::create_dir_all(&root)?;
    let photos_root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../assets/sample");

    let mut project = Project::new("Street / Walk 26");
    project.sheet.title = "Walk 26 / selects".into();
    project.sheet.columns = 3;
    project.sheet.rows = 4;
    project.sheet.rotate = false;

    for index in 0..12u32 {
        let path = photos_root.join(format!("street-walk-{:02}.jpg", index + 1));
        let (width, height) = image::image_dimensions(&path)?;
        let id = project.add_photo(path.clone(), width, height, file_size(&path));
        let tags: &[&str] = match index % 3 {
            0 => &["street", "night"],
            1 => &["street", "portrait"],
            _ => &["walk", "film"],
        };
        if let Some(photo) = project.photo_mut(id) {
            photo.set_tags(tags.iter().map(|tag| tag.to_string()).collect());
        }
    }

    let ids = project.photo_ids();
    let _ = batch::add_tag(&mut project, &ids[0..3], "select");

    let project_path = root.join("sample.slate");
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

fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}
