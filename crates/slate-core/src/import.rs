//! Import discovery and image decoding.
//!
//! Standard photo formats decode through the `image` crate. HEIC/HEIF and
//! camera RAW rely on macOS ImageIO via `sips`, which also bakes in orientation.
//! A file that cannot be decoded still enters the project: it shows as a
//! placeholder in the library and in exported sheets, so a batch is never
//! silently lost.

use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result};
use exif::{In, Reader as ExifReader, Tag};
use image::{DynamicImage, ImageReader, RgbaImage, imageops};

pub const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "gif", "bmp", "heic", "heif", "dng", "cr2", "cr3",
    "nef", "arw", "raf", "rw2", "orf", "pef", "srw", "3fr",
];

const STANDARD_EXTENSIONS: &[&str] = &["jpg", "jpeg", "png", "tif", "tiff", "webp", "gif", "bmp"];

#[derive(Debug, Clone)]
pub struct PreviewImage {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
}

impl PreviewImage {
    pub fn aspect(&self) -> f32 {
        if self.height == 0 {
            1.0
        } else {
            self.width as f32 / self.height as f32
        }
    }

    /// GPUI expects BGRA pixels.
    pub fn to_bgra(&self) -> Vec<u8> {
        let mut bgra = self.rgba.clone();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        bgra
    }

    pub fn rotated_clockwise(&self) -> Self {
        let Some(buffer) = RgbaImage::from_raw(self.width, self.height, self.rgba.clone()) else {
            return self.clone();
        };
        let rotated = imageops::rotate90(&buffer);
        Self {
            width: rotated.width(),
            height: rotated.height(),
            rgba: rotated.into_raw(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Probe {
    /// Orientation-corrected pixel dimensions.
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    pub preview: PreviewImage,
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

fn is_standard(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .map(|extension| STANDARD_EXTENSIONS.contains(&extension.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Expand files and directories into a deterministic, deduplicated list.
pub fn discover_images(inputs: &[PathBuf]) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for input in inputs {
        if input.is_dir() {
            walk(input, &mut found, 0);
        } else if input.is_file() && is_supported(input) {
            found.push(input.clone());
        }
    }

    found.sort_by(|left, right| {
        let left_name = left
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        let right_name = right
            .file_name()
            .map(|name| name.to_string_lossy().to_ascii_lowercase())
            .unwrap_or_default();
        left_name
            .cmp(&right_name)
            .then_with(|| left.to_string_lossy().cmp(&right.to_string_lossy()))
    });
    found.dedup();
    found
}

fn walk(directory: &Path, found: &mut Vec<PathBuf>, depth: usize) {
    if depth > 32 {
        return;
    }
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
    for entry in entries {
        let path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        if file_type.is_dir() {
            walk(&path, found, depth + 1);
        } else if file_type.is_file() && is_supported(&path) {
            found.push(path);
        }
    }
}

pub fn file_size(path: &Path) -> u64 {
    std::fs::metadata(path).map(|meta| meta.len()).unwrap_or(0)
}

pub fn probe(path: &Path, max_edge: u32) -> Result<Probe> {
    let bytes = file_size(path);
    let image = load_oriented(path, Some(max_edge))
        .with_context(|| format!("could not decode {}", path.display()))?;
    let (width, height) = original_dimensions(path).unwrap_or((image.width(), image.height()));
    let preview = to_preview(&image);
    Ok(Probe {
        width,
        height,
        bytes,
        preview,
    })
}

pub fn preview(path: &Path, max_edge: u32) -> Result<PreviewImage> {
    load_oriented(path, Some(max_edge))
        .map(|image| to_preview(&image))
        .with_context(|| format!("could not decode {}", path.display()))
}

/// Decode a photo with EXIF orientation applied. `max_edge` caps the long edge
/// so a contact sheet export never embeds more pixels than print resolution
/// needs.
pub fn load_oriented(path: &Path, max_edge: Option<u32>) -> Result<DynamicImage> {
    let mut image = if is_standard(path) {
        let reader = ImageReader::open(path)?
            .with_guessed_format()
            .with_context(|| format!("unrecognised image format for {}", path.display()))?;
        reader.decode()?
    } else {
        decode_via_sips(path, max_edge.unwrap_or(4096))?
    };

    if is_standard(path) {
        let orientation = read_orientation(path);
        image = apply_orientation(image, orientation);
    }

    if let Some(max_edge) = max_edge {
        let (width, height) = (image.width(), image.height());
        let long_edge = width.max(height);
        if long_edge > max_edge && long_edge > 0 {
            let scale = max_edge as f32 / long_edge as f32;
            let new_width = ((width as f32 * scale).round() as u32).max(1);
            let new_height = ((height as f32 * scale).round() as u32).max(1);
            // RAW files are the reason this path exists and they are not already
            // resized by `sips`; keep the filter cheap for large batches.
            image = image.resize_exact(new_width, new_height, imageops::FilterType::Triangle);
        }
    }

    Ok(image)
}

fn to_preview(image: &DynamicImage) -> PreviewImage {
    let rgba = composite_on_white(image.to_rgba8());
    PreviewImage {
        width: rgba.width(),
        height: rgba.height(),
        rgba: rgba.into_raw(),
    }
}

fn composite_on_white(mut rgba: RgbaImage) -> RgbaImage {
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
    rgba
}

fn read_orientation(path: &Path) -> u8 {
    let Ok(file) = File::open(path) else {
        return 1;
    };
    let mut reader = BufReader::new(file);
    let Ok(exif) = ExifReader::new().read_from_container(&mut reader) else {
        return 1;
    };
    exif.get_field(Tag::Orientation, In::PRIMARY)
        .and_then(|field| field.value.get_uint(0))
        .filter(|orientation| (1..=8).contains(orientation))
        .unwrap_or(1) as u8
}

fn apply_orientation(image: DynamicImage, orientation: u8) -> DynamicImage {
    match orientation {
        2 => image.fliph(),
        3 => image.rotate180(),
        4 => image.flipv(),
        5 => image.rotate90().fliph(),
        6 => image.rotate90(),
        7 => image.rotate90().flipv(),
        8 => image.rotate270(),
        _ => image,
    }
}

/// The real pixel size, before any preview downscale.
fn original_dimensions(path: &Path) -> Option<(u32, u32)> {
    if is_standard(path) {
        let reader = ImageReader::open(path).ok()?.with_guessed_format().ok()?;
        let (width, height) = reader.into_dimensions().ok()?;
        let orientation = read_orientation(path);
        return Some(if orientation >= 5 {
            (height, width)
        } else {
            (width, height)
        });
    }
    sips_dimensions(path).ok()
}

#[cfg(target_os = "macos")]
fn decode_via_sips(path: &Path, max_edge: u32) -> Result<DynamicImage> {
    let output = std::env::temp_dir().join(format!(
        "contact-sheet-sips-{}-{}.jpg",
        std::process::id(),
        unique()
    ));
    let status = std::process::Command::new("sips")
        .args(["-s", "format", "jpeg", "-Z", &max_edge.to_string()])
        .arg(path)
        .arg("--out")
        .arg(&output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .context("could not run sips")?;
    if !status.success() {
        anyhow::bail!("ImageIO could not read this file (HEIC/RAW support requires macOS)");
    }
    let result = ImageReader::open(&output)
        .map_err(anyhow::Error::from)
        .and_then(|reader| reader.decode().map_err(anyhow::Error::from));
    let _ = std::fs::remove_file(&output);
    result
}

#[cfg(not(target_os = "macos"))]
fn decode_via_sips(_path: &Path, _max_edge: u32) -> Result<DynamicImage> {
    anyhow::bail!("HEIC/RAW decoding is only available on macOS")
}

#[cfg(target_os = "macos")]
fn sips_dimensions(path: &Path) -> Result<(u32, u32)> {
    let output = std::process::Command::new("sips")
        .args(["-g", "pixelWidth", "-g", "pixelHeight"])
        .arg(path)
        .output()
        .context("could not run sips")?;
    if !output.status.success() {
        anyhow::bail!("could not read dimensions");
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut width = None;
    let mut height = None;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().parse::<u32>().ok();
        match key.trim() {
            "pixelWidth" => width = value,
            "pixelHeight" => height = value,
            _ => {}
        }
    }
    match (width, height) {
        (Some(width), Some(height)) if width > 0 && height > 0 => Ok((width, height)),
        _ => anyhow::bail!("sips returned no dimensions"),
    }
}

#[cfg(not(target_os = "macos"))]
fn sips_dimensions(_path: &Path) -> Result<(u32, u32)> {
    anyhow::bail!("sips is only available on macOS")
}

#[cfg(target_os = "macos")]
fn unique() -> u64 {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};

    fn write_test_jpeg(path: &Path, width: u32, height: u32) {
        let image: ImageBuffer<Rgb<u8>, _> = ImageBuffer::from_fn(width, height, |x, y| {
            Rgb([(x % 255) as u8, (y % 255) as u8, 90])
        });
        image.save(path).unwrap();
    }

    #[test]
    fn discovers_only_images_recursively() {
        let directory = tempfile::tempdir().unwrap();
        let nested = directory.path().join("nested");
        std::fs::create_dir(&nested).unwrap();
        write_test_jpeg(&directory.path().join("b.jpg"), 10, 10);
        write_test_jpeg(&nested.join("a.webp"), 10, 10);
        std::fs::write(directory.path().join("notes.txt"), "no").unwrap();
        std::fs::write(nested.join(".hidden.jpg"), "no").unwrap();

        let found = discover_images(&[directory.path().to_path_buf()]);
        assert_eq!(found.len(), 2);
        assert!(found[0].ends_with("a.webp"));
        assert!(found[1].ends_with("b.jpg"));
    }

    #[test]
    fn probes_png_dimensions_and_preview() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("photo.png");
        let image: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_fn(400, 200, |x, _| Rgb([(x % 255) as u8, 20, 30]));
        image.save(&path).unwrap();

        let probe = probe(&path, 64).unwrap();
        assert_eq!((probe.width, probe.height), (400, 200));
        assert_eq!(probe.preview.width, 64);
        assert_eq!(probe.preview.height, 32);
        assert_eq!(probe.preview.rgba.len(), 64 * 32 * 4);
        assert!(probe.bytes > 0);
    }

    #[test]
    fn rotates_previews() {
        let preview = PreviewImage {
            width: 2,
            height: 1,
            rgba: vec![
                255, 0, 0, 255, 0, 255, 0, 255, // red, green
            ],
        };
        let rotated = preview.rotated_clockwise();
        assert_eq!((rotated.width, rotated.height), (1, 2));
        assert_eq!(&rotated.rgba[0..4], &[255, 0, 0, 255]);
        assert_eq!(&rotated.rgba[4..8], &[0, 255, 0, 255]);
    }

    #[test]
    fn unsupported_extension_is_ignored() {
        assert!(!is_supported(Path::new("notes.txt")));
        assert!(is_supported(Path::new("photo.CR3")));
        assert!(is_supported(Path::new("photo.heic")));
    }
}
