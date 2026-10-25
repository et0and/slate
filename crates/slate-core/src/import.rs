//! Import discovery and image decoding.
//!
//! Standard photo formats decode through the `image` crate. HEIC/HEIF and
//! camera RAW rely on macOS ImageIO via `sips`, which also bakes in orientation.
//! A file that cannot be decoded still enters the project: it shows as a
//! placeholder in the library and in exported sheets, so a batch is never
//! silently lost.

use std::fs::File;
use std::io::{BufReader, Cursor};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result};
use chrono::{DateTime, Local};
use exif::{In, Reader as ExifReader, Tag};
use image::{DynamicImage, ImageReader, RgbaImage, imageops};
use jpeg_decoder::{Decoder as JpegDecoder, PixelFormat};

pub const EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "gif", "bmp", "dds", "exr", "hdr", "ico", "pnm",
    "qoi", "tga", "psd", "heic", "heif", "dng", "cr2", "cr3", "nef", "arw", "raf", "rw2", "orf",
    "pef", "srw", "3fr",
];

const STANDARD_EXTENSIONS: &[&str] = &[
    "jpg", "jpeg", "png", "tif", "tiff", "webp", "gif", "bmp", "dds", "exr", "hdr", "ico", "pnm",
    "qoi", "tga", "psd",
];

#[derive(Debug, Clone)]
pub struct PreviewImage {
    image: RgbaImage,
}

impl PreviewImage {
    pub fn width(&self) -> u32 {
        self.image.width()
    }

    pub fn height(&self) -> u32 {
        self.image.height()
    }

    /// GPUI expects BGRA pixels.
    pub fn into_bgra(self) -> Vec<u8> {
        let mut bgra = self.image.into_raw();
        for pixel in bgra.as_chunks_mut::<4>().0 {
            pixel.swap(0, 2);
        }
        bgra
    }

    fn from_dynamic_image(image: DynamicImage) -> Self {
        Self {
            image: composite_on_white(image.to_rgba8()),
        }
    }
}

#[derive(Debug, Clone)]
pub struct Probe {
    /// Orientation-corrected pixel dimensions.
    pub width: u32,
    pub height: u32,
    pub bytes: u64,
    /// EXIF capture time, or the file modified time when EXIF is absent.
    pub captured_at: Option<String>,
}

pub fn is_supported(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

fn is_standard(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            STANDARD_EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
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

    found.sort_by_cached_key(|path| {
        (
            path.file_name()
                .map(|name| name.to_string_lossy().to_ascii_lowercase())
                .unwrap_or_default(),
            path.to_string_lossy().into_owned(),
        )
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
    entries.sort_by_cached_key(|entry| entry.file_name().to_string_lossy().to_ascii_lowercase());
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

/// Read file and EXIF metadata without decoding the image.
///
/// Thumbnails are decoded lazily when a cell becomes visible. This keeps a
/// 500k-photo import from doing 500k image decodes whose previews would be
/// evicted by the bounded thumbnail cache.
#[tracing::instrument(level = "trace", skip_all)]
pub fn probe(path: &Path) -> Result<Probe> {
    let bytes = file_size(path);
    let (width, height) = original_dimensions(path)
        .with_context(|| format!("could not read dimensions for {}", path.display()))?;
    Ok(Probe {
        width,
        height,
        bytes,
        captured_at: capture_datetime(path),
    })
}

#[tracing::instrument(level = "trace", skip_all, fields(max_edge))]
pub fn preview(path: &Path, max_edge: u32) -> Result<PreviewImage> {
    load_oriented(path, Some(max_edge))
        .map(PreviewImage::from_dynamic_image)
        .with_context(|| format!("could not decode {}", path.display()))
}

/// Decode an in-memory thumbnail, such as a Google Drive preview.
pub fn preview_bytes(bytes: &[u8], max_edge: u32) -> Result<PreviewImage> {
    let image = image::load_from_memory(bytes).context("could not decode the thumbnail")?;
    Ok(PreviewImage::from_dynamic_image(resize_to_edge(
        image, max_edge,
    )))
}

/// Decode a photo with EXIF orientation applied. `max_edge` caps the long edge
/// so a contact sheet export never embeds more pixels than print resolution
/// needs.
#[tracing::instrument(level = "trace", skip_all, fields(max_edge = ?max_edge))]
pub fn load_oriented(path: &Path, max_edge: Option<u32>) -> Result<DynamicImage> {
    let mut image = if is_jpeg(path) {
        match decode_jpeg(path, max_edge) {
            Ok(Some(image)) => image,
            Ok(None) => decode_standard(path)?,
            Err(error) => return Err(error),
        }
    } else if is_psd(path) {
        decode_psd(path)?
    } else if is_standard(path) {
        decode_standard(path)?
    } else {
        decode_via_sips(path, max_edge.unwrap_or(4096))?
    };

    if is_standard(path) && !is_psd(path) {
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

fn is_jpeg(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            extension.eq_ignore_ascii_case("jpg") || extension.eq_ignore_ascii_case("jpeg")
        })
}

fn is_psd(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| extension.eq_ignore_ascii_case("psd"))
}

fn decode_psd(path: &Path) -> Result<DynamicImage> {
    let bytes = std::fs::read(path)?;
    let psd = psd::Psd::from_bytes(&bytes).map_err(anyhow::Error::from)?;
    let (width, height) = (psd.width(), psd.height());
    let pixels = psd.rgba();
    let image = RgbaImage::from_raw(width, height, pixels)
        .context("PSD pixel buffer does not match its dimensions")?;
    Ok(DynamicImage::ImageRgba8(image))
}

fn decode_standard(path: &Path) -> Result<DynamicImage> {
    let reader = ImageReader::open(path)?
        .with_guessed_format()
        .with_context(|| format!("unrecognised image format for {}", path.display()))?;
    Ok(reader.decode()?)
}

/// Decode JPEGs with the decoder's reduced IDCT. A 24 MP photo becomes a
/// few hundred kilobytes instead of a full RGBA frame, which keeps the
/// thumbnail path below the memory budget.
fn decode_jpeg(path: &Path, max_edge: Option<u32>) -> Result<Option<DynamicImage>> {
    let file = File::open(path)?;
    let mut decoder = JpegDecoder::new(BufReader::new(file));
    decoder.read_info()?;
    let info = decoder.info().context("JPEG has no image info")?;
    if !matches!(info.pixel_format, PixelFormat::RGB24 | PixelFormat::L8) {
        return Ok(None);
    }

    let (width, height) = (u32::from(info.width), u32::from(info.height));
    if let Some(max_edge) = max_edge {
        let long_edge = width.max(height);
        if long_edge > max_edge && long_edge > 0 {
            let scale = max_edge as f32 / long_edge as f32;
            let requested_width = ((width as f32 * scale).round() as u16).max(1);
            let requested_height = ((height as f32 * scale).round() as u16).max(1);
            decoder.scale(requested_width, requested_height)?;
        }
    }

    let pixels = decoder.decode()?;
    let info = decoder.info().context("JPEG has no decoded image info")?;
    let (width, height) = (u32::from(info.width), u32::from(info.height));
    let image = match info.pixel_format {
        PixelFormat::RGB24 => {
            image::RgbImage::from_raw(width, height, pixels).map(DynamicImage::ImageRgb8)
        }
        PixelFormat::L8 => {
            image::GrayImage::from_raw(width, height, pixels).map(DynamicImage::ImageLuma8)
        }
        _ => None,
    };
    Ok(image)
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

/// Read-only EXIF metadata for the properties panel. The order is stable and
/// labels are presentation-ready; no pixels are decoded.
#[derive(Debug, Clone, Default)]
pub struct PhotoMetadata {
    pub rows: Vec<(String, String)>,
}

pub fn read_metadata(path: &Path) -> PhotoMetadata {
    let Ok(file) = File::open(path) else {
        return PhotoMetadata::default();
    };
    let mut reader = BufReader::new(file);
    let Ok(exif) = ExifReader::new().read_from_container(&mut reader) else {
        return PhotoMetadata::default();
    };
    let mut rows = Vec::new();
    let make = field_value(&exif, Tag::Make);
    let model = field_value(&exif, Tag::Model);
    let camera = match (make, model) {
        (Some(make), Some(model)) if !model.starts_with(&make) => Some(format!("{make} {model}")),
        (Some(make), Some(model)) => Some(format!("{make} {model}")),
        (Some(make), None) => Some(make),
        (None, Some(model)) => Some(model),
        (None, None) => None,
    };
    push_row(&mut rows, "Camera", camera);
    push_row(&mut rows, "Lens", field_value(&exif, Tag::LensModel));
    push_row(
        &mut rows,
        "Captured",
        [Tag::DateTimeOriginal, Tag::DateTime]
            .into_iter()
            .find_map(|tag| field_value(&exif, tag).and_then(|value| normalize_datetime(&value))),
    );
    push_row(
        &mut rows,
        "ISO",
        field_value(&exif, Tag::PhotographicSensitivity)
            .or_else(|| field_value(&exif, Tag::ISOSpeed)),
    );
    push_row(
        &mut rows,
        "Aperture",
        field_value(&exif, Tag::FNumber).map(|value| format!("f/{value}")),
    );
    push_row(
        &mut rows,
        "Shutter",
        field_value(&exif, Tag::ExposureTime).map(|value| format!("{value} s")),
    );
    push_row(
        &mut rows,
        "Focal length",
        field_value(&exif, Tag::FocalLength),
    );
    push_row(&mut rows, "Software", field_value(&exif, Tag::Software));
    PhotoMetadata { rows }
}

fn field_value(exif: &exif::Exif, tag: Tag) -> Option<String> {
    let field = exif.get_field(tag, In::PRIMARY)?;
    let value = field.display_value().to_string();
    let value = value.trim();
    (!value.is_empty()).then(|| value.to_string())
}

fn push_row(rows: &mut Vec<(String, String)>, label: &str, value: Option<String>) {
    if let Some(value) = value {
        rows.push((label.to_string(), value));
    }
}

pub fn normalize_capture_datetime(value: &str) -> Option<String> {
    normalize_datetime(value)
}

pub fn capture_datetime(path: &Path) -> Option<String> {
    read_exif_datetime(path).or_else(|| {
        let modified = std::fs::metadata(path).ok()?.modified().ok()?;
        let local: DateTime<Local> = modified.into();
        Some(local.format("%Y-%m-%d %H:%M:%S").to_string())
    })
}

fn read_exif_datetime(path: &Path) -> Option<String> {
    let file = File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let exif = ExifReader::new().read_from_container(&mut reader).ok()?;
    [Tag::DateTimeOriginal, Tag::DateTime]
        .into_iter()
        .find_map(|tag| {
            exif.get_field(tag, In::PRIMARY)
                .and_then(|field| normalize_datetime(&field.display_value().to_string()))
        })
}

fn normalize_datetime(value: &str) -> Option<String> {
    let value = value.trim();
    let date = value.get(0..10)?;
    let time = value.get(11..19)?;
    if !date
        .bytes()
        .enumerate()
        .all(|(index, byte)| matches!(index, 4 | 7) || byte.is_ascii_digit())
        || !time
            .bytes()
            .enumerate()
            .all(|(index, byte)| matches!(index, 2 | 5) || byte.is_ascii_digit())
    {
        return None;
    }
    Some(format!(
        "{}-{}-{} {}:{}:{}",
        &date[0..4],
        &date[5..7],
        &date[8..10],
        &time[0..2],
        &time[3..5],
        &time[6..8]
    ))
}

fn resize_to_edge(image: DynamicImage, max_edge: u32) -> DynamicImage {
    let (width, height) = (image.width(), image.height());
    let long_edge = width.max(height);
    if long_edge <= max_edge || long_edge == 0 {
        return image;
    }
    let scale = max_edge as f32 / long_edge as f32;
    let width = ((width as f32 * scale).round() as u32).max(1);
    let height = ((height as f32 * scale).round() as u32).max(1);
    image.resize_exact(width, height, imageops::FilterType::Triangle)
}

pub(crate) fn read_orientation(path: &Path) -> u8 {
    let Ok(file) = File::open(path) else {
        return 1;
    };
    orientation_from_reader(&mut BufReader::new(file))
}

/// Read EXIF orientation from already-loaded image bytes.
pub(crate) fn orientation_from_bytes(bytes: &[u8]) -> u8 {
    orientation_from_reader(&mut BufReader::new(Cursor::new(bytes)))
}

fn orientation_from_reader<R: std::io::BufRead + std::io::Seek>(reader: &mut R) -> u8 {
    let Ok(exif) = ExifReader::new().read_from_container(reader) else {
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
    if is_psd(path) {
        let bytes = std::fs::read(path).ok()?;
        let psd = psd::Psd::from_bytes(&bytes).ok()?;
        return Some((psd.width(), psd.height()));
    }
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

    #[test]
    fn normalizes_exif_capture_times() {
        assert_eq!(
            normalize_datetime("2025:08:17 09:41:03").as_deref(),
            Some("2025-08-17 09:41:03")
        );
        assert_eq!(
            normalize_datetime("2025-08-17 09:41:03").as_deref(),
            Some("2025-08-17 09:41:03")
        );
        assert_eq!(normalize_datetime("not a date"), None);
    }

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
    fn metadata_is_empty_without_exif() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("plain.jpg");
        write_test_jpeg(&path, 20, 20);
        assert!(read_metadata(&path).rows.is_empty());
    }

    #[test]
    fn probes_png_dimensions_without_decoding() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("photo.png");
        let image: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_fn(400, 200, |x, _| Rgb([(x % 255) as u8, 20, 30]));
        image.save(&path).unwrap();

        let probe = probe(&path).unwrap();
        assert_eq!((probe.width, probe.height), (400, 200));
        assert!(probe.bytes > 0);
    }

    #[test]
    fn probes_jpeg_dimensions_without_decoding() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("photo.jpg");
        write_test_jpeg(&path, 400, 200);
        let probe = probe(&path).unwrap();
        assert_eq!((probe.width, probe.height), (400, 200));
    }

    #[test]
    fn unsupported_extension_is_ignored() {
        assert!(!is_supported(Path::new("notes.txt")));
        assert!(is_supported(Path::new("photo.CR3")));
        assert!(is_supported(Path::new("photo.heic")));
        assert!(!is_supported(Path::new("texture.ktx")));
        assert!(is_supported(Path::new("texture.dds")));
        assert!(is_supported(Path::new("design.psd")));
        assert!(is_supported(Path::new("icon.ico")));
    }
}
