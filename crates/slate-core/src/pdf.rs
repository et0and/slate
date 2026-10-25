//! Flat PDF export.
//!
//! The same page planner that drives the in-app preview drives the PDF. Pages
//! are written to disk one at a time; the document is never held in memory as a
//! whole, which is required for a 500k-photo sheet.

use std::fs::File;
use std::io::{BufWriter, Cursor, Write};
use std::path::{Path, PathBuf};
#[cfg(target_os = "macos")]
use std::sync::atomic::{AtomicU64, Ordering};

use anyhow::{Context as _, Result};
use image::codecs::jpeg::JpegEncoder;
use image::{DynamicImage, RgbImage};
use jpeg_decoder::{Decoder as JpegDecoder, PixelFormat};
#[cfg(target_os = "macos")]
use objc2_core_foundation::{
    CFBoolean, CFDictionary, CFMutableData, CFNumber, CFString, CFType, CFURL, CFURLPathStyle,
};
#[cfg(target_os = "macos")]
use objc2_image_io::{
    CGImageDestination, CGImageSource, kCGImageSourceCreateThumbnailFromImageAlways,
    kCGImageSourceCreateThumbnailWithTransform, kCGImageSourceThumbnailMaxPixelSize,
};
use rayon::prelude::*;

use crate::import::{load_oriented, orientation_from_bytes};
use crate::layout::{MM_TO_PT, Placement, SheetLayout};
use crate::model::{Photo, PhotoId, PhotoSet, Project, SheetSettings};

const DPI: f32 = 300.0;
const TEXT_BLACK: (f32, f32, f32) = (0.04, 0.04, 0.04);
const TEXT_GREY: (f32, f32, f32) = (0.42, 0.42, 0.42);
const HAIRLINE: (f32, f32, f32) = (0.78, 0.77, 0.74);
const JPEG_QUALITY: u8 = 85;

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

fn pdf_string(value: &str) -> String {
    let mut escaped = String::with_capacity(value.len());
    for character in value.chars() {
        match character {
            '(' | ')' | '\\' => {
                escaped.push('\\');
                escaped.push(character);
            }
            character if character.is_ascii() && !character.is_control() => {
                escaped.push(character);
            }
            _ => escaped.push('?'),
        }
    }
    escaped
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

struct PdfWriter {
    file: BufWriter<File>,
    offsets: Vec<u64>,
    length: u64,
}

impl PdfWriter {
    fn new(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("could not create {}", parent.display()))?;
        }
        let file =
            File::create(path).with_context(|| format!("could not create {}", path.display()))?;
        let mut writer = Self {
            file: BufWriter::new(file),
            offsets: vec![0],
            length: 0,
        };
        writer.write(b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n")?;
        Ok(writer)
    }

    fn write(&mut self, bytes: &[u8]) -> Result<()> {
        self.file.write_all(bytes)?;
        self.length += bytes.len() as u64;
        Ok(())
    }

    fn alloc(&mut self) -> u32 {
        let id = self.offsets.len() as u32;
        self.offsets.push(0);
        id
    }

    fn object(&mut self, id: u32, body: &[u8]) -> Result<()> {
        self.offsets[id as usize] = self.length;
        self.write(format!("{id} 0 obj\n<< ").as_bytes())?;
        self.write(body)?;
        self.write(b" >>\nendobj\n")
    }

    fn stream(&mut self, id: u32, dict: &str, data: &[u8]) -> Result<()> {
        self.offsets[id as usize] = self.length;
        self.write(
            format!("{id} 0 obj\n<< {dict} /Length {} >>\nstream\n", data.len()).as_bytes(),
        )?;
        self.write(data)?;
        self.write(b"\nendstream\nendobj\n")
    }

    fn finish(mut self, catalog: u32, pages: u32, kids: &[u32]) -> Result<usize> {
        let mut body = format!("/Type /Pages /Count {} /Kids [", kids.len());
        for id in kids {
            body.push_str(&format!("{id} 0 R "));
        }
        body.push(']');
        self.object(pages, body.as_bytes())?;

        // PDF 1.5 xref streams use explicit 8-byte offsets. The classic xref
        // table is limited to 10 decimal digits, which a 500k export can exceed.
        let xref_id = self.alloc();
        let size = self.offsets.len();
        let xref_offset = self.length;
        self.offsets[xref_id as usize] = xref_offset;

        let mut entries = Vec::with_capacity(size * 11);
        entries.extend_from_slice(&[0]);
        entries.extend_from_slice(&0u64.to_be_bytes());
        entries.extend_from_slice(&65535u16.to_be_bytes());
        for offset in self.offsets.iter().skip(1) {
            entries.push(1);
            entries.extend_from_slice(&offset.to_be_bytes());
            entries.extend_from_slice(&0u16.to_be_bytes());
        }

        self.write(
            format!(
                "{xref_id} 0 obj\n<< /Type /XRef /Size {size} /W [1 8 2] /Root {catalog} 0 R /Length {} >>\nstream\n",
                entries.len()
            )
            .as_bytes(),
        )?;
        self.write(&entries)?;
        self.write(format!("\nendstream\nendobj\nstartxref\n{xref_offset}\n%%EOF\n").as_bytes())?;
        self.file.flush()?;
        Ok(self.length as usize)
    }
}

struct PageContent {
    ops: String,
    images: Vec<(String, u32)>,
}

impl PageContent {
    fn new() -> Self {
        Self {
            ops: String::new(),
            images: Vec::new(),
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn text(
        &mut self,
        page_h_mm: f32,
        x_mm: f32,
        top_mm: f32,
        size: f32,
        font: &str,
        fill: (f32, f32, f32),
        value: &str,
    ) {
        let x = mm_to_pt(x_mm);
        let y = mm_to_pt(top_to_bottom(page_h_mm, top_mm));
        use std::fmt::Write as _;
        let _ = writeln!(
            self.ops,
            "BT\n/{font} {size} Tf\n{} {} {} rg\n1 0 0 1 {x:.3} {y:.3} Tm\n({}) Tj\nET",
            fill.0,
            fill.1,
            fill.2,
            pdf_string(value)
        );
    }

    fn image(&mut self, placement: &Placement, page_h_mm: f32, image_id: u32) {
        self.image_with_transform(placement, page_h_mm, image_id, UnitTransform::IDENTITY);
    }

    fn image_with_transform(
        &mut self,
        placement: &Placement,
        page_h_mm: f32,
        image_id: u32,
        transform: UnitTransform,
    ) {
        let name = format!("Im{}", self.images.len());
        let x = mm_to_pt(placement.image_x_mm);
        let y = mm_to_pt(top_to_bottom(
            page_h_mm,
            placement.image_y_mm + placement.image_h_mm,
        ));
        let width = mm_to_pt(placement.image_w_mm);
        let height = mm_to_pt(placement.image_h_mm);
        let (a, b, c, d, e, f) = transform.pdf_matrix(x, y, width, height);
        use std::fmt::Write as _;
        let _ = writeln!(
            self.ops,
            "q\n{a:.3} {b:.3} {c:.3} {d:.3} {e:.3} {f:.3} cm\n/{name} Do\nQ"
        );
        self.images.push((name, image_id));
    }

    fn rectangle(&mut self, page_h_mm: f32, x_mm: f32, top_mm: f32, width_mm: f32, height_mm: f32) {
        let x = mm_to_pt(x_mm);
        let y = mm_to_pt(top_to_bottom(page_h_mm, top_mm + height_mm));
        let width = mm_to_pt(width_mm);
        let height = mm_to_pt(height_mm);
        use std::fmt::Write as _;
        let _ = writeln!(
            self.ops,
            "{} {} {} RG\n0.4 w\n{x:.3} {y:.3} {width:.3} {height:.3} re\nS",
            HAIRLINE.0, HAIRLINE.1, HAIRLINE.2
        );
    }

    fn hairline(&mut self, page_h_mm: f32, x1_mm: f32, y1_mm: f32, x2_mm: f32, y2_mm: f32) {
        let x1 = mm_to_pt(x1_mm);
        let y1 = mm_to_pt(top_to_bottom(page_h_mm, y1_mm));
        let x2 = mm_to_pt(x2_mm);
        let y2 = mm_to_pt(top_to_bottom(page_h_mm, y2_mm));
        use std::fmt::Write as _;
        let _ = writeln!(
            self.ops,
            "{} {} {} RG\n0.4 w\n{x1:.3} {y1:.3} m\n{x2:.3} {y2:.3} l\nS",
            HAIRLINE.0, HAIRLINE.1, HAIRLINE.2
        );
    }

    fn resource_dictionary(&self, fonts: (u32, u32, u32)) -> String {
        let mut resources = format!(
            "/Font << /F1 {} 0 R /F2 {} 0 R /F3 {} 0 R >>",
            fonts.0, fonts.1, fonts.2
        );
        if self.images.is_empty() {
            resources.push_str(" /XObject << >>");
        } else {
            resources.push_str(" /XObject << ");
            for (name, id) in &self.images {
                resources.push_str(&format!("/{name} {id} 0 R "));
            }
            resources.push_str(">>");
        }
        resources
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
struct UnitTransform {
    a: f32,
    b: f32,
    c: f32,
    d: f32,
    e: f32,
    f: f32,
}

impl UnitTransform {
    const IDENTITY: Self = Self {
        a: 1.0,
        b: 0.0,
        c: 0.0,
        d: 1.0,
        e: 0.0,
        f: 0.0,
    };

    fn rotated_clockwise(self) -> Self {
        Self {
            a: self.b,
            b: -self.a,
            c: self.d,
            d: -self.c,
            e: self.f,
            f: 1.0 - self.e,
        }
    }

    fn pdf_matrix(self, x: f32, y: f32, width: f32, height: f32) -> (f32, f32, f32, f32, f32, f32) {
        (
            width * self.a,
            height * self.b,
            width * self.c,
            height * self.d,
            x + width * self.e,
            y + height * self.f,
        )
    }
}

/// Unit-square transform that matches `import::apply_orientation`.
fn exif_transform(orientation: u8) -> Option<UnitTransform> {
    Some(match orientation {
        1 => UnitTransform::IDENTITY,
        2 => UnitTransform {
            a: -1.0,
            b: 0.0,
            c: 0.0,
            d: 1.0,
            e: 1.0,
            f: 0.0,
        },
        3 => UnitTransform {
            a: -1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: 1.0,
            f: 1.0,
        },
        4 => UnitTransform {
            a: 1.0,
            b: 0.0,
            c: 0.0,
            d: -1.0,
            e: 0.0,
            f: 1.0,
        },
        5 => UnitTransform {
            a: 0.0,
            b: -1.0,
            c: -1.0,
            d: 0.0,
            e: 1.0,
            f: 1.0,
        },
        6 => UnitTransform {
            a: 0.0,
            b: 1.0,
            c: -1.0,
            d: 0.0,
            e: 0.0,
            f: 1.0,
        },
        7 => UnitTransform {
            a: 0.0,
            b: 1.0,
            c: 1.0,
            d: 0.0,
            e: 0.0,
            f: 0.0,
        },
        8 => UnitTransform {
            a: 0.0,
            b: -1.0,
            c: 1.0,
            d: 0.0,
            e: 1.0,
            f: 0.0,
        },
        _ => return None,
    })
}

fn displayed_transform(
    photo: &Photo,
    placement: &Placement,
    orientation: u8,
) -> Option<UnitTransform> {
    let mut transform = exif_transform(orientation)?;
    let turns = photo
        .rotation()
        .turns()
        .wrapping_add(u8::from(placement.rotated));
    for _ in 0..turns {
        transform = transform.rotated_clockwise();
    }
    Some(transform)
}

#[derive(Debug, Clone, Copy)]
enum JpegColor {
    Rgb,
    Gray,
    Cmyk { inverted: bool },
}

impl JpegColor {
    fn device_space(self) -> &'static str {
        match self {
            Self::Rgb => "/DeviceRGB",
            Self::Gray => "/DeviceGray",
            Self::Cmyk { .. } => "/DeviceCMYK",
        }
    }

    fn decode_parameter(self) -> &'static str {
        if let Self::Cmyk { inverted: true } = self {
            " /Decode [1 0 1 0 1 0 1 0]"
        } else {
            ""
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct JpegInfo {
    width: u32,
    height: u32,
    color: JpegColor,
    orientation: u8,
}

fn jpeg_info(bytes: &[u8]) -> Option<JpegInfo> {
    let mut decoder = JpegDecoder::new(Cursor::new(bytes));
    decoder.read_info().ok()?;
    let info = decoder.info()?;
    let color = match info.pixel_format {
        PixelFormat::RGB24 => JpegColor::Rgb,
        PixelFormat::L8 => JpegColor::Gray,
        PixelFormat::CMYK32 => match adobe_transform(bytes) {
            Some(2) => return None,
            Some(_) => JpegColor::Cmyk { inverted: true },
            None => JpegColor::Cmyk { inverted: false },
        },
        _ => return None,
    };
    Some(JpegInfo {
        width: u32::from(info.width),
        height: u32::from(info.height),
        color,
        orientation: orientation_from_bytes(bytes),
    })
}

/// Adobe APP14 transform byte. `0` is inverted CMYK, `2` is YCCK.
fn adobe_transform(bytes: &[u8]) -> Option<u8> {
    if bytes.len() < 4 || bytes[0] != 0xFF || bytes[1] != 0xD8 {
        return None;
    }
    let mut index = 2;
    while index + 4 <= bytes.len() {
        if bytes[index] != 0xFF {
            break;
        }
        let marker = bytes[index + 1];
        index += 2;
        if marker == 0xD8 || (0xD0..=0xD7).contains(&marker) {
            continue;
        }
        if marker == 0xDA || marker == 0xD9 || index + 2 > bytes.len() {
            break;
        }
        let length = u16::from_be_bytes([bytes[index], bytes[index + 1]]) as usize;
        if length < 2 || index + length > bytes.len() {
            break;
        }
        if marker == 0xEE && length >= 14 && &bytes[index + 2..index + 7] == b"Adobe" {
            return Some(bytes[index + 13]);
        }
        index += length;
    }
    None
}

fn jpeg_dict(info: JpegInfo) -> String {
    format!(
        "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8{} /Filter /DCTDecode",
        info.width,
        info.height,
        info.color.device_space(),
        info.color.decode_parameter()
    )
}

struct RawImage {
    bytes: Vec<u8>,
    dict: String,
    transform: UnitTransform,
}

fn raw_jpeg(photo: &Photo, placement: &Placement) -> Option<RawImage> {
    if !matches!(photo.extension().as_str(), "jpg" | "jpeg") {
        return None;
    }
    let bytes = std::fs::read(&photo.path).ok()?;
    let info = jpeg_info(&bytes)?;
    let transform = displayed_transform(photo, placement, info.orientation)?;
    Some(RawImage {
        bytes,
        dict: jpeg_dict(info),
        transform,
    })
}

#[derive(Debug, Clone, Copy)]
enum PngColor {
    Gray,
    Rgb,
}

impl PngColor {
    fn device_space(self) -> &'static str {
        match self {
            Self::Gray => "/DeviceGray",
            Self::Rgb => "/DeviceRGB",
        }
    }

    fn components(self) -> u8 {
        match self {
            Self::Gray => 1,
            Self::Rgb => 3,
        }
    }
}

struct PngInfo {
    width: u32,
    height: u32,
    color: PngColor,
    data: Vec<u8>,
    orientation: u8,
}

fn png_info(bytes: &[u8]) -> Option<PngInfo> {
    const SIGNATURE: &[u8; 8] = b"\x89PNG\r\n\x1a\n";
    if !bytes.starts_with(SIGNATURE) {
        return None;
    }
    let mut index = SIGNATURE.len();
    let mut header = None;
    let mut data = Vec::new();
    let mut has_trns = false;
    let mut saw_end = false;

    while index + 12 <= bytes.len() {
        let length = u32::from_be_bytes([
            bytes[index],
            bytes[index + 1],
            bytes[index + 2],
            bytes[index + 3],
        ]) as usize;
        let data_start = index + 8;
        let data_end = data_start.checked_add(length)?;
        if data_end + 4 > bytes.len() {
            return None;
        }
        match &bytes[index + 4..index + 8] {
            b"IHDR" => {
                if length != 13 {
                    return None;
                }
                let width = u32::from_be_bytes([
                    bytes[data_start],
                    bytes[data_start + 1],
                    bytes[data_start + 2],
                    bytes[data_start + 3],
                ]);
                let height = u32::from_be_bytes([
                    bytes[data_start + 4],
                    bytes[data_start + 5],
                    bytes[data_start + 6],
                    bytes[data_start + 7],
                ]);
                header = Some((
                    width,
                    height,
                    bytes[data_start + 8],
                    bytes[data_start + 9],
                    bytes[data_start + 12],
                ));
            }
            b"tRNS" => has_trns = true,
            b"IDAT" => data.extend_from_slice(&bytes[data_start..data_end]),
            b"IEND" => {
                saw_end = true;
                break;
            }
            _ => {}
        }
        index = data_end + 4;
    }

    if !saw_end || has_trns {
        return None;
    }
    let (width, height, bit_depth, color_type, interlace) = header?;
    if bit_depth != 8 || interlace != 0 {
        return None;
    }
    let color = match color_type {
        0 => PngColor::Gray,
        2 => PngColor::Rgb,
        _ => return None,
    };
    if data.is_empty() {
        return None;
    }
    Some(PngInfo {
        width,
        height,
        color,
        data,
        orientation: orientation_from_bytes(bytes),
    })
}

fn png_dict(info: &PngInfo) -> String {
    format!(
        "/Type /XObject /Subtype /Image /Width {} /Height {} /ColorSpace {} /BitsPerComponent 8 /Filter /FlateDecode /DecodeParms << /Predictor 15 /Colors {} /BitsPerComponent 8 /Columns {} >>",
        info.width,
        info.height,
        info.color.device_space(),
        info.color.components(),
        info.width
    )
}

fn raw_png(photo: &Photo, placement: &Placement) -> Option<RawImage> {
    if photo.extension() != "png" {
        return None;
    }
    let bytes = std::fs::read(&photo.path).ok()?;
    let info = png_info(&bytes)?;
    let transform = displayed_transform(photo, placement, info.orientation)?;
    Some(RawImage {
        dict: png_dict(&info),
        bytes: info.data,
        transform,
    })
}

fn max_pixels(placement: &Placement) -> u32 {
    (((placement.image_w_mm.max(placement.image_h_mm) * DPI) / 25.4).ceil() as u32)
        .max(64)
        .saturating_mul(2)
}

#[cfg(target_os = "macos")]
fn is_native_raw(photo: &Photo) -> bool {
    matches!(
        photo.extension().as_str(),
        "heic"
            | "heif"
            | "dng"
            | "cr2"
            | "cr3"
            | "nef"
            | "arw"
            | "raf"
            | "rw2"
            | "orf"
            | "pef"
            | "srw"
            | "3fr"
    )
}

#[cfg(target_os = "macos")]
fn image_io_raw(photo: &Photo, placement: &Placement) -> Option<RawImage> {
    if !is_native_raw(photo) {
        return None;
    }
    let path = photo.path.to_str()?;
    let path_string = CFString::from_str(path);
    let url = CFURL::with_file_system_path(
        None,
        Some(&path_string),
        CFURLPathStyle::CFURLPOSIXPathStyle,
        false,
    )?;
    let source = unsafe { CGImageSource::with_url(&url, None) }?;
    let max_pixels = max_pixels(placement);
    let options = unsafe {
        CFDictionary::<CFType, CFType>::from_slices(
            &[
                kCGImageSourceCreateThumbnailFromImageAlways.as_ref(),
                kCGImageSourceThumbnailMaxPixelSize.as_ref(),
                kCGImageSourceCreateThumbnailWithTransform.as_ref(),
            ],
            &[
                CFBoolean::new(true).as_ref(),
                CFNumber::new_isize(max_pixels as isize).as_ref(),
                CFBoolean::new(true).as_ref(),
            ],
        )
    };
    let image = unsafe { source.thumbnail_at_index(0, Some(options.as_ref())) }?;
    let data = CFMutableData::new(None, 0)?;
    let jpeg_type = CFString::from_str("public.jpeg");
    let destination = unsafe { CGImageDestination::with_data(&data, &jpeg_type, 1, None) }?;
    unsafe { destination.add_image(&image, None) };
    if !unsafe { destination.finalize() } {
        return None;
    }
    let bytes = data.to_vec();
    let info = jpeg_info(&bytes)?;
    let transform = displayed_transform(photo, placement, info.orientation)?;
    Some(RawImage {
        bytes,
        dict: jpeg_dict(info),
        transform,
    })
}

#[cfg(not(target_os = "macos"))]
fn image_io_raw(_photo: &Photo, _placement: &Placement) -> Option<RawImage> {
    None
}

#[cfg(target_os = "macos")]
static SIPS_COUNTER: AtomicU64 = AtomicU64::new(0);

#[cfg(target_os = "macos")]
fn sips_raw(photo: &Photo, placement: &Placement) -> Option<RawImage> {
    if !is_native_raw(photo) {
        return None;
    }
    let output = std::env::temp_dir().join(format!(
        "slate-pdf-sips-{}-{}.jpg",
        std::process::id(),
        SIPS_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let status = std::process::Command::new("sips")
        .args([
            "-s",
            "format",
            "jpeg",
            "-Z",
            &max_pixels(placement).to_string(),
        ])
        .arg(photo.path.as_ref())
        .arg("--out")
        .arg(&output)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .ok()?;
    if !status.success() {
        let _ = std::fs::remove_file(&output);
        return None;
    }
    let bytes = std::fs::read(&output).ok();
    let _ = std::fs::remove_file(&output);
    let bytes = bytes?;
    let info = jpeg_info(&bytes)?;
    let transform = displayed_transform(photo, placement, info.orientation)?;
    Some(RawImage {
        bytes,
        dict: jpeg_dict(info),
        transform,
    })
}

#[cfg(not(target_os = "macos"))]
fn sips_raw(_photo: &Photo, _placement: &Placement) -> Option<RawImage> {
    None
}

enum PreparedImage {
    Raw(RawImage),
    Encoded {
        bytes: Vec<u8>,
        width: u32,
        height: u32,
    },
    Missing(String),
}

fn prepare_photo(photo: &Photo, placement: &Placement) -> PreparedImage {
    if let Some(image) = raw_jpeg(photo, placement) {
        return PreparedImage::Raw(image);
    }
    if let Some(image) = raw_png(photo, placement) {
        return PreparedImage::Raw(image);
    }
    if let Some(image) = image_io_raw(photo, placement) {
        return PreparedImage::Raw(image);
    }
    if let Some(image) = sips_raw(photo, placement) {
        return PreparedImage::Raw(image);
    }
    match render_photo(photo, placement) {
        Ok((bytes, width, height)) => PreparedImage::Encoded {
            bytes,
            width,
            height,
        },
        Err(error) => PreparedImage::Missing(error.to_string()),
    }
}

fn flatten_on_white(image: DynamicImage) -> RgbImage {
    let rgba = image.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (source, target) in rgba.pixels().zip(rgb.pixels_mut()) {
        let alpha = u16::from(source[3]);
        if alpha < 255 {
            let blend =
                |channel: u8| ((u16::from(channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
            target[0] = blend(source[0]);
            target[1] = blend(source[1]);
            target[2] = blend(source[2]);
        } else {
            target[0] = source[0];
            target[1] = source[1];
            target[2] = source[2];
        }
    }
    rgb
}

fn render_photo(photo: &Photo, placement: &Placement) -> Result<(Vec<u8>, u32, u32)> {
    let max_px = (((placement.image_w_mm.max(placement.image_h_mm) * DPI) / 25.4).ceil() as u32)
        .max(64)
        .saturating_mul(2);
    let mut image = load_oriented(&photo.path, Some(max_px))?;
    for _ in 0..photo.rotation().turns() {
        image = image.rotate90();
    }
    if placement.rotated {
        image = image.rotate90();
    }

    let target_w = ((placement.image_w_mm * DPI) / 25.4).ceil() as u32;
    let target_h = ((placement.image_h_mm * DPI) / 25.4).ceil() as u32;
    let image = flatten_on_white(image.thumbnail(target_w.max(1), target_h.max(1)));
    let (width, height) = (image.width(), image.height());
    let mut bytes = Vec::new();
    let mut encoder = JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY);
    encoder.encode_image(&DynamicImage::ImageRgb8(image))?;
    Ok((bytes, width, height))
}

fn draw_header(
    content: &mut PageContent,
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
    content.text(
        layout.page_h_mm,
        layout.margin_mm,
        layout.margin_mm + 6.0,
        13.0,
        "F2",
        TEXT_BLACK,
        &title,
    );

    let meta = format!(
        "Contact sheet   {} photos   {} {}   {} x {}",
        photo_count,
        settings.paper.label(),
        settings.orientation.label(),
        settings.columns,
        settings.rows
    );
    content.text(
        layout.page_h_mm,
        layout.margin_mm,
        layout.margin_mm + 10.5,
        6.0,
        "F3",
        TEXT_GREY,
        &meta,
    );

    let page_label = format!("Sheet {page:02} / {page_count:02}");
    let field_width = layout.page_w_mm - layout.margin_mm * 2.0;
    let x = layout.margin_mm + field_width - page_label.chars().count() as f32 * 1.28;
    content.text(
        layout.page_h_mm,
        x,
        layout.margin_mm + 10.5,
        6.0,
        "F3",
        TEXT_GREY,
        &page_label,
    );

    content.hairline(
        layout.page_h_mm,
        layout.margin_mm,
        layout.content_top_mm - 3.0,
        layout.page_w_mm - layout.margin_mm,
        layout.content_top_mm - 3.0,
    );
}

fn draw_footer(
    content: &mut PageContent,
    layout: &SheetLayout,
    project_name: &str,
    page: usize,
    page_count: usize,
) {
    let rule_top = layout.page_h_mm - layout.margin_mm - 7.0;
    content.hairline(
        layout.page_h_mm,
        layout.margin_mm,
        rule_top,
        layout.page_w_mm - layout.margin_mm,
        rule_top,
    );

    let label = ascii_lossy(project_name);
    let label_width = layout.page_w_mm - layout.margin_mm * 2.0 - 40.0;
    let label = truncate_courier(&label, label_width, 6.0);
    content.text(
        layout.page_h_mm,
        layout.margin_mm,
        rule_top + 4.0,
        6.0,
        "F3",
        TEXT_GREY,
        &label,
    );

    let page_label = format!("Page {page:02} / {page_count:02}");
    let x = layout.margin_mm + (layout.page_w_mm - layout.margin_mm * 2.0)
        - page_label.chars().count() as f32 * 1.28;
    content.text(
        layout.page_h_mm,
        x,
        rule_top + 4.0,
        6.0,
        "F3",
        TEXT_GREY,
        &page_label,
    );
}

fn draw_caption(
    content: &mut PageContent,
    layout: &SheetLayout,
    placement: &Placement,
    caption: &str,
) {
    let caption_top = placement.cell_y_mm + layout.image_h_mm;
    content.hairline(
        layout.page_h_mm,
        placement.cell_x_mm,
        caption_top,
        placement.cell_x_mm + layout.cell_w_mm,
        caption_top,
    );
    let value = truncate_courier(caption, layout.cell_w_mm, 6.0);
    content.text(
        layout.page_h_mm,
        placement.cell_x_mm + 0.5,
        caption_top + 3.6,
        6.0,
        "F3",
        TEXT_GREY,
        &value,
    );
}

#[allow(clippy::too_many_arguments)]
fn write_page(
    writer: &mut PdfWriter,
    settings: &SheetSettings,
    layout: &SheetLayout,
    page: &crate::layout::PagePlan,
    page_photos: &[&Photo],
    project_name: &str,
    page_count: usize,
    photo_count: usize,
    fonts: (u32, u32, u32),
    pages: u32,
    missing: &mut Vec<String>,
) -> Result<u32> {
    let mut content = PageContent::new();
    if settings.header {
        draw_header(
            &mut content,
            layout,
            settings,
            project_name,
            page.number,
            page_count,
            photo_count,
        );
    }
    if page.placements.is_empty() {
        content.text(
            layout.page_h_mm,
            layout.margin_mm,
            layout.content_top_mm + 10.0,
            7.0,
            "F3",
            TEXT_GREY,
            "No photos selected",
        );
    }
    let prepared: Vec<PreparedImage> = page
        .placements
        .par_iter()
        .map(|placement| {
            let Some(photo) = page_photos.get(placement.photo_index) else {
                return PreparedImage::Missing("photo not found".to_string());
            };
            prepare_photo(photo, placement)
        })
        .collect();

    for (placement, prepared) in page.placements.iter().zip(prepared) {
        match prepared {
            PreparedImage::Raw(image) => {
                let image_id = writer.alloc();
                writer.stream(image_id, &image.dict, &image.bytes)?;
                content.image_with_transform(
                    placement,
                    layout.page_h_mm,
                    image_id,
                    image.transform,
                );
            }
            PreparedImage::Encoded {
                bytes,
                width,
                height,
            } => {
                let image_id = writer.alloc();
                writer.stream(
                    image_id,
                    &format!(
                        "/Type /XObject /Subtype /Image /Width {width} /Height {height} /ColorSpace /DeviceRGB /BitsPerComponent 8 /Filter /DCTDecode"
                    ),
                    &bytes,
                )?;
                content.image(placement, layout.page_h_mm, image_id);
            }
            PreparedImage::Missing(error) => {
                content.rectangle(
                    layout.page_h_mm,
                    placement.image_x_mm,
                    placement.image_y_mm,
                    placement.image_w_mm,
                    placement.image_h_mm,
                );
                content.text(
                    layout.page_h_mm,
                    placement.image_x_mm + 4.0,
                    placement.image_y_mm + placement.image_h_mm / 2.0,
                    6.0,
                    "F3",
                    TEXT_GREY,
                    "Missing",
                );
                let name = page_photos
                    .get(placement.photo_index)
                    .map(|photo| photo.file_name())
                    .unwrap_or_else(|| "unknown photo".to_string());
                missing.push(format!("{name}: {error}"));
            }
        }
        if let Some(caption) = &placement.caption {
            draw_caption(&mut content, layout, placement, caption);
        }
    }
    if settings.footer {
        draw_footer(&mut content, layout, project_name, page.number, page_count);
    }
    let content_id = writer.alloc();
    writer.stream(content_id, "", content.ops.as_bytes())?;
    let page_id = writer.alloc();
    let width = mm_to_pt(layout.page_w_mm);
    let height = mm_to_pt(layout.page_h_mm);
    writer.object(
        page_id,
        format!(
            "/Type /Page /Parent {pages} 0 R /MediaBox [0 0 {width:.3} {height:.3}] /Resources << {} /ProcSet [/PDF /Text /ImageC] >> /Contents {content_id} 0 R",
            content.resource_dictionary(fonts)
        )
        .as_bytes(),
    )?;
    Ok(page_id)
}

/// Render selected photos into a flat PDF at `output`.
///
/// Pages are planned, rendered and written one at a time. Only the current
/// page's pixels and JPEG bytes are live, so export memory is bounded by the
/// page size instead of the library size.
pub fn export_pdf(
    project_name: &str,
    settings: &SheetSettings,
    photos: &PhotoSet,
    output: &Path,
) -> Result<ExportSummary> {
    let layout = SheetLayout::from_settings(settings);
    let per_page = layout.per_page().max(1);
    let page_count: usize = photos.len().div_ceil(per_page).max(1);
    let mut writer = PdfWriter::new(output)?;

    let catalog = writer.alloc();
    let pages = writer.alloc();
    let regular = writer.alloc();
    let bold = writer.alloc();
    let mono = writer.alloc();
    writer.object(
        catalog,
        format!("/Type /Catalog /Pages {pages} 0 R").as_bytes(),
    )?;
    writer.object(
        regular,
        b"/Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding",
    )?;
    writer.object(
        bold,
        b"/Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding",
    )?;
    writer.object(
        mono,
        b"/Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding",
    )?;

    let mut kids = Vec::with_capacity(page_count);
    let mut missing = Vec::new();
    for page_index in 0..page_count {
        let start = page_index.saturating_mul(per_page);
        let end = (start + per_page).min(photos.len());
        let page_photos: Vec<&Photo> = (start..end).filter_map(|index| photos.get(index)).collect();
        let page =
            crate::layout::plan_page_refs(settings, &layout, &page_photos, start, page_index);
        let page_id = write_page(
            &mut writer,
            settings,
            &layout,
            &page,
            &page_photos,
            project_name,
            page_count,
            photos.len(),
            (regular, bold, mono),
            pages,
            &mut missing,
        )?;
        kids.push(page_id);
    }

    let bytes = writer.finish(catalog, pages, &kids)?;
    Ok(ExportSummary {
        path: output.to_path_buf(),
        pages: page_count,
        photos: photos.len(),
        missing,
        bytes,
        warnings: 0,
    })
}

/// Render a scope of photo ids into a flat PDF without cloning every photo.
pub fn export_pdf_ids(
    project_name: &str,
    settings: &SheetSettings,
    project: &Project,
    ids: &[PhotoId],
    output: &Path,
) -> Result<ExportSummary> {
    let layout = SheetLayout::from_settings(settings);
    let per_page = layout.per_page().max(1);
    let page_count: usize = ids.len().div_ceil(per_page).max(1);
    let mut writer = PdfWriter::new(output)?;
    let catalog = writer.alloc();
    let pages = writer.alloc();
    let regular = writer.alloc();
    let bold = writer.alloc();
    let mono = writer.alloc();
    writer.object(
        catalog,
        format!("/Type /Catalog /Pages {pages} 0 R").as_bytes(),
    )?;
    writer.object(
        regular,
        b"/Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding",
    )?;
    writer.object(
        bold,
        b"/Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding",
    )?;
    writer.object(
        mono,
        b"/Type /Font /Subtype /Type1 /BaseFont /Courier /Encoding /WinAnsiEncoding",
    )?;

    let mut kids = Vec::with_capacity(page_count);
    let mut missing = Vec::new();
    for page_index in 0..page_count {
        let start = page_index.saturating_mul(per_page);
        let end = (start + per_page).min(ids.len());
        let page_ids = &ids[start..end];
        let page_photos: Vec<&Photo> = page_ids
            .iter()
            .filter_map(|id| project.photo(*id))
            .collect();
        if page_photos.len() != page_ids.len() {
            missing.push(format!(
                "{} photos in the project could not be resolved",
                page_ids.len() - page_photos.len()
            ));
        }
        let page =
            crate::layout::plan_page_refs(settings, &layout, &page_photos, start, page_index);
        let page_id = write_page(
            &mut writer,
            settings,
            &layout,
            &page,
            &page_photos,
            project_name,
            page_count,
            ids.len(),
            (regular, bold, mono),
            pages,
            &mut missing,
        )?;
        kids.push(page_id);
    }
    let bytes = writer.finish(catalog, pages, &kids)?;
    Ok(ExportSummary {
        path: output.to_path_buf(),
        pages: page_count,
        photos: ids.len(),
        missing,
        bytes,
        warnings: 0,
    })
}

/// Export a saved sheet by resolving its stored photo ids.
pub fn export_saved_sheet(
    project: &crate::model::Project,
    sheet: &crate::model::SavedSheet,
    output: &Path,
) -> Result<ExportSummary> {
    export_pdf_ids(
        &project.name,
        &sheet.settings,
        project,
        &sheet.photo_ids,
        output,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Paper, PhotoId, PhotoSize, Project};
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
        Photo::new(PhotoId(id), path, PhotoSize::new(width, height), 1)
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
        let photos =
            PhotoSet::from_vec(vec![photo(1, first, 600, 400), photo(2, second, 400, 600)])
                .unwrap();

        let summary = export_pdf("Test", &settings, &photos, &output).unwrap();
        assert_eq!(summary.pages, 1);
        assert_eq!(summary.photos, 2);
        assert!(summary.missing.is_empty());
        let bytes = std::fs::read(&output).unwrap();
        assert!(bytes.starts_with(b"%PDF-"));
        assert!(bytes.len() > 4_000, "pdf should contain image data");
        assert!(
            bytes
                .windows(b"/Width 600 /Height 400".len())
                .any(|window| window == b"/Width 600 /Height 400"),
            "JPEG photos should be embedded without resizing"
        );
        assert_eq!(summary.bytes, bytes.len());
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        assert_eq!(document.get_pages().len(), 1);
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

        let project_path = directory.path().join("saved.slate");
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
    fn png_photos_use_the_original_flate_stream() {
        let directory = tempfile::tempdir().unwrap();
        let path = image_path(directory.path(), "a.png", 300, 200);
        let mut project = Project::new("PNG");
        let id = project.add_photo(path, 300, 200, 1);
        let output = directory.path().join("png.pdf");
        let summary =
            export_pdf_ids("PNG", &SheetSettings::default(), &project, &[id], &output).unwrap();
        assert_eq!(summary.photos, 1);
        let bytes = std::fs::read(&output).unwrap();
        assert!(
            bytes
                .windows(b"/Filter /FlateDecode".len())
                .any(|window| window == b"/Filter /FlateDecode")
        );
        assert!(
            bytes
                .windows(b"/Predictor 15".len())
                .any(|window| window == b"/Predictor 15")
        );
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        assert_eq!(document.get_pages().len(), 1);
    }

    #[test]
    fn exports_ids_without_cloning_photos() {
        let directory = tempfile::tempdir().unwrap();
        let path = image_path(directory.path(), "a.jpg", 200, 100);
        let mut project = Project::new("Ids");
        let first = project.add_photo(path.clone(), 200, 100, 1);
        let second = project.add_photo(path, 200, 100, 1);
        let output = directory.path().join("ids.pdf");
        let summary = export_pdf_ids(
            "Ids",
            &SheetSettings::default(),
            &project,
            &[first, second],
            &output,
        )
        .unwrap();
        assert_eq!(summary.pages, 1);
        assert_eq!(summary.photos, 2);
        let bytes = std::fs::read(&output).unwrap();
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        assert_eq!(document.get_pages().len(), 1);
    }

    #[test]
    fn missing_photo_becomes_a_placeholder_not_a_failure() {
        let directory = tempfile::tempdir().unwrap();
        let output = directory.path().join("missing.pdf");
        let photos =
            PhotoSet::from_vec(vec![photo(1, directory.path().join("gone.jpg"), 100, 100)])
                .unwrap();
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
        let photos = PhotoSet::from_vec(
            (0..3)
                .map(|index| photo(index + 1, path.clone(), 100, 100))
                .collect(),
        )
        .unwrap();
        let output = directory.path().join("three.pdf");
        let summary = export_pdf("Test", &settings, &photos, &output).unwrap();
        assert_eq!(summary.pages, 3);
        let bytes = std::fs::read(&output).unwrap();
        let document = lopdf::Document::load_mem(&bytes).unwrap();
        assert_eq!(document.get_pages().len(), 3);
    }

    #[test]
    fn exif_orientations_map_to_signed_transforms() {
        assert_eq!(exif_transform(1), Some(UnitTransform::IDENTITY));
        assert_eq!(exif_transform(2).unwrap().a, -1.0);
        assert_eq!(exif_transform(3).unwrap().d, -1.0);
        assert_eq!(exif_transform(4).unwrap().d, -1.0);
        assert_eq!(exif_transform(5).unwrap().b, -1.0);
        assert_eq!(exif_transform(6).unwrap().c, -1.0);
        assert_eq!(exif_transform(7).unwrap().c, 1.0);
        assert_eq!(exif_transform(8).unwrap().b, -1.0);
        assert_eq!(exif_transform(9), None);
        let turned = exif_transform(1).unwrap().rotated_clockwise();
        assert_eq!(turned.a, 0.0);
        assert_eq!(turned.b, -1.0);
        assert_eq!(turned.c, 1.0);
        assert_eq!(turned.f, 1.0);
    }

    #[test]
    fn adobe_app14_transform_is_detected() {
        let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xEE, 0x00, 0x0E];
        bytes.extend_from_slice(b"Adobe");
        bytes.extend_from_slice(&[0, 100, 0, 0, 0, 0, 0]);
        assert_eq!(adobe_transform(&bytes), Some(0));
        bytes[17] = 2;
        assert_eq!(adobe_transform(&bytes), Some(2));
        let plain = [0xFF, 0xD8, 0xFF, 0xE0, 0x00, 0x04, 0, 0];
        assert_eq!(adobe_transform(&plain), None);
    }

    #[test]
    fn jpeg_info_reads_dimensions_and_color() {
        let directory = tempfile::tempdir().unwrap();
        let path = image_path(directory.path(), "info.jpg", 600, 400);
        let bytes = std::fs::read(path).unwrap();
        let info = jpeg_info(&bytes).unwrap();
        assert_eq!((info.width, info.height), (600, 400));
        assert!(matches!(info.color, JpegColor::Rgb));
        assert_eq!(info.orientation, 1);
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
