//! Batch conversion for the formats the bundled `image` codecs support.

use std::io::BufWriter;
use std::path::{Path, PathBuf};

use image::{DynamicImage, ImageFormat, RgbImage, RgbaImage, codecs::jpeg::JpegEncoder};

use crate::import;
use crate::model::{PhotoId, Project};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Jpeg,
    Png,
    WebP,
    Tiff,
}

impl OutputFormat {
    pub const ALL: [Self; 4] = [Self::Jpeg, Self::Png, Self::WebP, Self::Tiff];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Jpeg => "JPEG",
            Self::Png => "PNG",
            Self::WebP => "WebP",
            Self::Tiff => "TIFF",
        }
    }

    pub const fn extension(self) -> &'static str {
        match self {
            Self::Jpeg => "jpg",
            Self::Png => "png",
            Self::WebP => "webp",
            Self::Tiff => "tiff",
        }
    }

    fn image_format(self) -> ImageFormat {
        match self {
            Self::Jpeg => ImageFormat::Jpeg,
            Self::Png => ImageFormat::Png,
            Self::WebP => ImageFormat::WebP,
            Self::Tiff => ImageFormat::Tiff,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct ConvertOptions {
    pub format: OutputFormat,
    pub quality: u8,
}

impl Default for ConvertOptions {
    fn default() -> Self {
        Self {
            format: OutputFormat::Jpeg,
            quality: 92,
        }
    }
}

#[derive(Debug, Default)]
pub struct ConvertReport {
    pub converted: usize,
    pub skipped: usize,
    pub errors: Vec<(String, String)>,
    pub outputs: Vec<PathBuf>,
}

impl ConvertReport {
    pub fn summary(&self) -> String {
        format!(
            "Converted {} / skipped {}{}",
            self.converted,
            self.skipped,
            if self.errors.is_empty() {
                String::new()
            } else {
                format!(" / {} error(s)", self.errors.len())
            }
        )
    }
}

#[tracing::instrument(skip_all, fields(photos = ids.len(), format = ?options.format))]
pub fn convert_photos(
    project: &Project,
    ids: &[PhotoId],
    output_dir: &Path,
    options: ConvertOptions,
) -> ConvertReport {
    let mut report = ConvertReport::default();
    if let Err(error) = std::fs::create_dir_all(output_dir) {
        report.errors.push((
            output_dir.display().to_string(),
            format!("could not create output folder: {error}"),
        ));
        return report;
    }

    for id in ids {
        let Some(photo) = project.photo(*id) else {
            report.skipped += 1;
            continue;
        };
        let Some(stem) = photo.path.file_stem().and_then(|stem| stem.to_str()) else {
            report.errors.push((
                photo.file_name(),
                "could not read the source file name".to_string(),
            ));
            continue;
        };
        let target = output_dir.join(format!("{stem}.{}", options.format.extension()));
        if target.exists() {
            report.skipped += 1;
            continue;
        }
        match convert_one(&photo.path, &target, options) {
            Ok(()) => {
                report.converted += 1;
                report.outputs.push(target);
            }
            Err(error) => {
                report.errors.push((photo.file_name(), error.to_string()));
            }
        }
    }
    report
}

fn convert_one(source: &Path, target: &Path, options: ConvertOptions) -> anyhow::Result<()> {
    let image = import::load_oriented(source, None)?;
    match options.format {
        OutputFormat::Jpeg => {
            let image = flatten_on_white(image);
            let file = std::fs::File::create(target)?;
            let mut encoder =
                JpegEncoder::new_with_quality(BufWriter::new(file), options.quality.clamp(1, 100));
            encoder.encode_image(&image)?;
        }
        _ => {
            image.save_with_format(target, options.format.image_format())?;
        }
    }
    Ok(())
}

fn flatten_on_white(image: DynamicImage) -> RgbImage {
    let rgba: RgbaImage = image.to_rgba8();
    let mut rgb = RgbImage::new(rgba.width(), rgba.height());
    for (x, y, pixel) in rgba.enumerate_pixels() {
        let alpha = u16::from(pixel[3]);
        let blend = |channel: u8| ((u16::from(channel) * alpha + 255 * (255 - alpha)) / 255) as u8;
        rgb.put_pixel(
            x,
            y,
            image::Rgb([blend(pixel[0]), blend(pixel[1]), blend(pixel[2])]),
        );
    }
    rgb
}

#[cfg(test)]
mod tests {
    use super::*;
    use image::{ImageBuffer, Rgb};
    use tempfile::tempdir;

    #[test]
    fn converts_to_the_requested_formats_without_clobbering() {
        let directory = tempdir().unwrap();
        let source = directory.path().join("source.png");
        let image: ImageBuffer<Rgb<u8>, _> =
            ImageBuffer::from_fn(8, 6, |x, y| Rgb([x as u8 * 10, y as u8 * 10, 90]));
        image.save(&source).unwrap();
        let mut project = Project::new("Convert");
        let id = project.add_photo(source.clone(), 8, 6, 1);
        let output = directory.path().join("out");
        for format in OutputFormat::ALL {
            let report = convert_photos(
                &project,
                &[id],
                &output,
                ConvertOptions {
                    format,
                    quality: 90,
                },
            );
            assert_eq!(report.converted, 1, "{format:?}: {:?}", report.errors);
            let target = output.join(format!("source.{}", format.extension()));
            assert!(target.exists());
            assert!(image::open(&target).is_ok());
        }
        let report = convert_photos(&project, &[id], &output, ConvertOptions::default());
        assert_eq!(report.converted, 0);
        assert_eq!(report.skipped, 1);
    }
}
