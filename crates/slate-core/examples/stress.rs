//! Measures project load, page planning and batch planning without the UI.
//!
//! `cargo run --release -p slate-core --example stress -- [path.slate]`
//! `cargo run --release -p slate-core --example stress -- --photos 500000 --gate`
//!
//! `--gate` fails when the 500k profile exceeds the checked limits. The
//! release workflow runs that form on Linux.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::ensure;
use slate_core::Project;
use slate_core::batch::{RenameOptions, plan_rename};
use slate_core::layout::plan_sheet;

const GATE_PHOTOS: usize = 500_000;
const GATE_FILE_MB: f64 = 160.0;
const GATE_LOAD: Duration = Duration::from_secs(15);
const GATE_PLAN: Duration = Duration::from_secs(3);
const GATE_GENERATE: Duration = Duration::from_secs(120);
const GATE_RENAME: Duration = Duration::from_secs(60);
const GATE_RSS_MB: f64 = 256.0;

fn main() -> anyhow::Result<()> {
    let mut path = None;
    let mut photos = 10_000;
    let mut gate = false;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--photos" => {
                photos = args
                    .next()
                    .ok_or_else(|| anyhow::anyhow!("--photos needs a count"))?
                    .parse()?;
            }
            "--gate" => gate = true,
            other => path = Some(PathBuf::from(other)),
        }
    }
    let started = Instant::now();
    let (path, generated) = if let Some(path) = path {
        (path, None)
    } else {
        let path = generate(photos)?;
        (path, Some(started.elapsed()))
    };
    let bytes = std::fs::metadata(&path)?.len();
    let file_mb = bytes as f64 / (1024.0 * 1024.0);

    let started = Instant::now();
    let project = Project::load(&path)?;
    let load = started.elapsed();
    let base_rss_mb = current_rss_mb();

    let started = Instant::now();
    let plan = plan_sheet(&project.sheet, &project.photos);
    let plan_time = started.elapsed();

    let started = Instant::now();
    let clone = project.clone();
    let clone_time = started.elapsed();
    drop(clone);

    let ids = project.photo_ids();
    let options = RenameOptions {
        prefix: "bench-".into(),
        suffix: String::new(),
        start: 1,
        padding: 6,
    };
    let started = Instant::now();
    let rename = plan_rename(&project, &ids, &options);
    let rename_time = started.elapsed();
    let peak_rss_mb = current_rss_mb();

    println!("project: {}", path.display());
    println!("photos: {}", project.photos.len());
    println!("file: {file_mb:.1} MB");
    println!("pages: {}", plan.pages.len());
    if let Some(generated) = generated {
        println!("generate: {:.1} ms", generated.as_secs_f64() * 1000.0);
    }
    println!("load: {:.1} ms", load.as_secs_f64() * 1000.0);
    println!("plan: {:.1} ms", plan_time.as_secs_f64() * 1000.0);
    println!("clone: {:.2} ms", clone_time.as_secs_f64() * 1000.0);
    println!(
        "rename plan: {} entries / {} conflicts / {:.1} ms",
        rename.entries.len(),
        rename.conflicts,
        rename_time.as_secs_f64() * 1000.0
    );
    if let Some(rss_mb) = base_rss_mb {
        println!("rss after load: {rss_mb:.1} MB");
    }
    if let Some(rss_mb) = peak_rss_mb {
        println!("rss after plan+rename: {rss_mb:.1} MB");
    }

    if gate {
        ensure!(project.photos.len() == GATE_PHOTOS, "expected 500k photos");
        ensure!(
            file_mb <= GATE_FILE_MB,
            "project file exceeds {GATE_FILE_MB} MB"
        );
        ensure!(load <= GATE_LOAD, "project load exceeds {GATE_LOAD:?}");
        ensure!(
            plan_time <= GATE_PLAN,
            "sheet planning exceeds {GATE_PLAN:?}"
        );
        if let Some(generated) = generated {
            ensure!(
                generated <= GATE_GENERATE,
                "generation exceeds {GATE_GENERATE:?}"
            );
        }
        ensure!(
            rename_time <= GATE_RENAME,
            "rename planning exceeds {GATE_RENAME:?}"
        );
        if let Some(rss_mb) = base_rss_mb {
            ensure!(
                rss_mb <= GATE_RSS_MB,
                "resident set after load exceeds {GATE_RSS_MB} MB"
            );
        }
        println!("stress gate: PASS");
    }
    Ok(())
}

fn generate(count: usize) -> anyhow::Result<PathBuf> {
    let image = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../assets/sample/street-walk-01.jpg")
        .canonicalize()?;
    let bytes = std::fs::metadata(&image)?.len();
    let mut project = Project::new(format!("{count} photos"));
    project.sheet.title = "Stress sheet".into();
    for _ in 0..count {
        project.add_photo(image.clone(), 1200, 1501, bytes);
    }
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target/stress");
    std::fs::create_dir_all(&directory)?;
    let path = directory.join(format!("{count}.slate"));
    project.save(&path)?;
    Ok(path)
}

#[cfg(unix)]
fn current_rss_mb() -> Option<f64> {
    let output = std::process::Command::new("ps")
        .args(["-o", "rss=", "-p", &std::process::id().to_string()])
        .output()
        .ok()?;
    let kilobytes = String::from_utf8_lossy(&output.stdout)
        .trim()
        .parse::<f64>()
        .ok()?;
    Some(kilobytes / 1024.0)
}

#[cfg(not(unix))]
const fn current_rss_mb() -> Option<f64> {
    None
}
