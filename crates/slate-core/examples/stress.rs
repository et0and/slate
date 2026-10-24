//! Measures project load and page planning without touching the UI.
//!
//! `cargo run --release -p slate-core --example stress -- [path.slate]`
//! With no path, writes a 10,000-photo project under `target/stress`.

use std::path::{Path, PathBuf};
use std::time::Instant;

use slate_core::Project;
use slate_core::batch::{RenameOptions, plan_rename};
use slate_core::layout::plan_sheet;

fn main() -> anyhow::Result<()> {
    let path = std::env::args().nth(1).map(PathBuf::from);
    let path = match path {
        Some(path) => path,
        None => generate()?,
    };
    let bytes = std::fs::metadata(&path)?.len();

    let started = Instant::now();
    let project = Project::load(&path)?;
    let load = started.elapsed();

    let started = Instant::now();
    let plan = plan_sheet(&project.sheet, &project.photos);
    let plan_time = started.elapsed();

    println!("project: {}", path.display());
    println!("file: {:.1} MB", bytes as f64 / (1024.0 * 1024.0));
    println!("photos: {}", project.photos.len());
    println!("pages: {}", plan.pages.len());
    println!("load: {:.1} ms", load.as_secs_f64() * 1000.0);
    println!("plan: {:.1} ms", plan_time.as_secs_f64() * 1000.0);

    let started = Instant::now();
    let clone = project.clone();
    println!("clone: {:.2} ms", started.elapsed().as_secs_f64() * 1000.0);
    drop(clone);

    let ids = project.photo_ids();
    let options = RenameOptions {
        prefix: "bench-".into(),
        suffix: String::new(),
        start: 1,
        padding: 5,
    };
    let started = Instant::now();
    let rename = plan_rename(&project, &ids, &options);
    println!(
        "rename plan: {} entries / {} conflicts / {:.1} ms",
        rename.entries.len(),
        rename.conflicts,
        started.elapsed().as_secs_f64() * 1000.0
    );
    Ok(())
}

fn generate() -> anyhow::Result<PathBuf> {
    let image = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/sample/street-walk-01.jpg")
        .canonicalize()?;
    let bytes = std::fs::metadata(&image)?.len();
    let mut project = Project::new("10,000 photos");
    project.sheet.title = "Stress sheet".into();
    for _ in 0..10_000 {
        project.add_photo(image.clone(), 1200, 1501, bytes);
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/stress");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join("ten-thousand.slate");
    project.save(&path)?;
    Ok(path)
}
