//! Video frames for storyboards.
//!
//! A video's length is divided into the requested number of equal parts and
//! one frame is taken from the middle of each part. Frames are written as
//! JPEGs into a folder beside the video, so the storyboard is an ordinary
//! folder collection and every photo workflow applies to it unchanged.

use std::fs::File;
use std::io::BufWriter;
use std::path::{Path, PathBuf};
use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use anyhow::{Context as _, Result, bail};
use ffmpeg_next as ffmpeg;
use image::RgbImage;
use image::codecs::jpeg::JpegEncoder;
use image::imageops;

use crate::model::QuarterTurns;

/// Containers offered by the file picker and recognised when dropped.
pub const EXTENSIONS: &[&str] = &[
    "mp4", "mov", "m4v", "mkv", "webm", "avi", "mts", "m2ts", "mpg", "mpeg", "wmv", "3gp", "mxf",
];
pub const MIN_FRAMES: u32 = 1;
pub const MAX_FRAMES: u32 = 999;
pub const DEFAULT_FRAMES: u32 = 12;
const JPEG_QUALITY: u8 = 92;

#[must_use]
pub fn is_video(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            EXTENSIONS
                .iter()
                .any(|candidate| extension.eq_ignore_ascii_case(candidate))
        })
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VideoInfo {
    pub duration: Duration,
    /// Coded size, before the display rotation.
    pub width: u32,
    pub height: u32,
    /// Frames per second, when the container states it.
    pub frame_rate: Option<f64>,
    /// Clockwise turns that display the video upright, as phones record
    /// portrait video sideways with a rotation flag.
    pub rotation: QuarterTurns,
}

impl VideoInfo {
    /// The size a frame is shown at, after rotation.
    #[must_use]
    pub const fn display_size(&self) -> (u32, u32) {
        if self.rotation.is_odd() {
            (self.height, self.width)
        } else {
            (self.width, self.height)
        }
    }
}

/// How large each written frame is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FrameSize {
    Original,
    /// Scale down so the longer edge is at most this many pixels.
    LongEdge(u32),
}

impl FrameSize {
    fn fit(self, (width, height): (u32, u32)) -> (u32, u32) {
        match self {
            Self::LongEdge(edge) if width.max(height) > edge => {
                let scale = f64::from(edge) / f64::from(width.max(height));
                (
                    ((f64::from(width) * scale).round() as u32).max(2) & !1,
                    ((f64::from(height) * scale).round() as u32).max(2) & !1,
                )
            }
            _ => (width, height),
        }
    }
}

#[derive(Debug, Clone)]
pub struct ExtractRequest {
    pub video: PathBuf,
    pub count: u32,
    pub size: FrameSize,
    /// Where the frames go; created if it does not exist.
    pub folder: PathBuf,
}

#[derive(Debug)]
pub enum ExtractOutcome {
    Finished(Vec<PathBuf>),
    /// Stopped on request; every frame written so far has been removed.
    Cancelled,
}

fn init() {
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Initialisation only fails on FFmpeg builds older than any we link.
        let _ = ffmpeg::init();
        ffmpeg::util::log::set_level(ffmpeg::util::log::Level::Error);
    });
}

/// The moment of each frame: the middle of each of `count` equal parts, so
/// the first and last frames avoid fades in and out.
#[must_use]
pub fn frame_times(duration: Duration, count: u32) -> Vec<Duration> {
    let count = count.clamp(MIN_FRAMES, MAX_FRAMES);
    (0..count)
        .map(|index| duration.mul_f64((f64::from(index) + 0.5) / f64::from(count)))
        .collect()
}

/// `Walk-007-01m12s345.jpg`: name order is time order, and the timecode is
/// readable wherever the file goes.
#[must_use]
pub fn frame_file_name(stem: &str, index: usize, count: u32, time: Duration) -> String {
    let digits = count.to_string().len().max(3);
    let millis = time.as_millis();
    let (hours, minutes, seconds, millis) = (
        millis / 3_600_000,
        millis / 60_000 % 60,
        millis / 1000 % 60,
        millis % 1000,
    );
    let timecode = if hours > 0 {
        format!("{hours}h{minutes:02}m{seconds:02}s{millis:03}")
    } else {
        format!("{minutes:02}m{seconds:02}s{millis:03}")
    };
    format!("{stem}-{:0digits$}-{timecode}.jpg", index + 1)
}

/// `Walk frames` beside `Walk.mov`, or `Walk frames 2` and so on when an
/// earlier storyboard already has the name. An empty folder is reused.
#[must_use]
pub fn frames_folder(video: &Path) -> PathBuf {
    let stem = video.file_stem().map_or_else(
        || "Video".to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let parent = video.parent().unwrap_or_else(|| Path::new("."));
    (1..10_000)
        .map(|index| {
            parent.join(if index == 1 {
                format!("{stem} frames")
            } else {
                format!("{stem} frames {index}")
            })
        })
        .find(|folder| {
            std::fs::read_dir(folder).map_or(true, |mut entries| entries.next().is_none())
        })
        .unwrap_or_else(|| parent.join(format!("{stem} frames")))
}

/// Clockwise quarter turns from a stream's display matrix, the 3 × 3 16.16
/// fixed-point transform `FFmpeg` stores for rotated video.
fn matrix_turns(matrix: &[u8]) -> QuarterTurns {
    let value = |index: usize| {
        matrix
            .get(index * 4..index * 4 + 4)
            .and_then(|bytes| <[u8; 4]>::try_from(bytes).ok())
            .map_or(0.0, |bytes| f64::from(i32::from_ne_bytes(bytes)) / 65536.0)
    };
    let (a, b, c, d) = (value(0), value(1), value(3), value(4));
    let (scale_x, scale_y) = (a.hypot(c), b.hypot(d));
    if scale_x == 0.0 || scale_y == 0.0 {
        return QuarterTurns::NONE;
    }
    let degrees = (b / scale_y).atan2(a / scale_x).to_degrees();
    let turns = (degrees / 90.0).round().rem_euclid(4.0) as u8;
    QuarterTurns::try_from(turns).unwrap_or(QuarterTurns::NONE)
}

/// An open video: the demuxer, its best video stream and that stream's
/// decoder.
struct Reader {
    input: ffmpeg::format::context::Input,
    stream: usize,
    time_base: f64,
    decoder: ffmpeg::decoder::Video,
    info: VideoInfo,
}

impl Reader {
    fn open(path: &Path) -> Result<Self> {
        init();
        let input = ffmpeg::format::input(path).context("could not open the video")?;
        let stream = input
            .streams()
            .best(ffmpeg::media::Type::Video)
            .context("the file has no video stream")?;
        let index = stream.index();
        let time_base = f64::from(stream.time_base());
        let decoder = ffmpeg::codec::context::Context::from_parameters(stream.parameters())
            .context("could not read the video stream")?
            .decoder()
            .video()
            .context("this video format is not supported")?;
        let container = input.duration();
        let duration = if container > 0 {
            container as f64 / f64::from(ffmpeg::ffi::AV_TIME_BASE)
        } else {
            stream.duration() as f64 * time_base
        };
        if !duration.is_finite() || duration <= 0.0 {
            bail!("the video does not state its length");
        }
        let rate = f64::from(stream.avg_frame_rate());
        let rotation = stream
            .side_data()
            .find(|data| data.kind() == ffmpeg::codec::packet::side_data::Type::DisplayMatrix)
            .map_or(QuarterTurns::NONE, |data| matrix_turns(data.data()));
        let info = VideoInfo {
            duration: Duration::from_secs_f64(duration),
            width: decoder.width(),
            height: decoder.height(),
            frame_rate: (rate.is_finite() && rate > 0.0).then_some(rate),
            rotation,
        };
        Ok(Self {
            input,
            stream: index,
            time_base,
            decoder,
            info,
        })
    }

    /// Decode the first frame at or after `time`. The demuxer jumps to the
    /// keyframe before it and decodes forward, so the frame is the one shown
    /// at that moment rather than the nearest keyframe.
    fn frame_at(
        &mut self,
        time: Duration,
        cancel: &AtomicBool,
    ) -> Result<Option<ffmpeg::frame::Video>> {
        let target = time.as_secs_f64();
        let micros = (target * f64::from(ffmpeg::ffi::AV_TIME_BASE)) as i64;
        self.input
            .seek(micros, ..micros)
            .context("could not move through the video")?;
        self.decoder.flush();
        let mut frame = ffmpeg::frame::Video::empty();
        let mut latest = None;
        for (stream, packet) in self.input.packets() {
            if cancel.load(Ordering::Relaxed) {
                return Ok(None);
            }
            if stream.index() != self.stream {
                continue;
            }
            self.decoder
                .send_packet(&packet)
                .context("could not decode the video")?;
            while self.decoder.receive_frame(&mut frame).is_ok() {
                let at = frame
                    .timestamp()
                    .map_or(target, |pts| pts as f64 * self.time_base);
                if at + 1e-3 >= target {
                    return Ok(Some(frame));
                }
                latest = Some(frame.clone());
            }
        }
        // Past the last packet: drain the decoder and take the final frame.
        let _ = self.decoder.send_eof();
        while self.decoder.receive_frame(&mut frame).is_ok() {
            latest = Some(frame.clone());
        }
        latest.map_or_else(
            || bail!("no frame decodes near {target:.2} s"),
            |frame| Ok(Some(frame)),
        )
    }
}

/// Read a video's length, size, frame rate and rotation.
///
/// # Errors
///
/// Fails when the file cannot be opened or has no decodable video stream.
pub fn probe(path: &Path) -> Result<VideoInfo> {
    Reader::open(path).map(|reader| reader.info)
}

/// Convert a decoded frame to upright RGB at `size`.
fn to_rgb(frame: &ffmpeg::frame::Video, info: &VideoInfo, size: FrameSize) -> Result<RgbImage> {
    // Scale in coded orientation, so a portrait clip's long edge is measured
    // after rotation.
    let (display_w, display_h) = size.fit(info.display_size());
    let (width, height) = if info.rotation.is_odd() {
        (display_h, display_w)
    } else {
        (display_w, display_h)
    };
    let mut scaler = ffmpeg::software::scaling::Context::get(
        frame.format(),
        frame.width(),
        frame.height(),
        ffmpeg::format::Pixel::RGB24,
        width,
        height,
        ffmpeg::software::scaling::Flags::BICUBIC,
    )
    .context("could not convert the frame")?;
    let mut rgb = ffmpeg::frame::Video::empty();
    scaler
        .run(frame, &mut rgb)
        .context("could not convert the frame")?;

    // FFmpeg pads rows; copy each row's pixels into a tight buffer.
    let stride = rgb.stride(0);
    let row = width as usize * 3;
    let data = rgb.data(0);
    let mut pixels = Vec::with_capacity(row * height as usize);
    for line in 0..height as usize {
        pixels.extend_from_slice(&data[line * stride..line * stride + row]);
    }
    let image = RgbImage::from_raw(width, height, pixels).context("the frame size is invalid")?;
    Ok(match info.rotation.turns() {
        1 => imageops::rotate90(&image),
        2 => imageops::rotate180(&image),
        3 => imageops::rotate270(&image),
        _ => image,
    })
}

fn write_jpeg(image: &RgbImage, path: &Path) -> Result<()> {
    let file = File::create_new(path).context("could not create the frame file")?;
    JpegEncoder::new_with_quality(BufWriter::new(file), JPEG_QUALITY)
        .encode_image(image)
        .context("could not write the frame")
}

/// Extract `request.count` frames into `request.folder`. `cancel` stops the
/// job between and inside frames; `done` counts frames written, for progress.
///
/// # Errors
///
/// Fails when the video cannot be read or a frame cannot be written. Frames
/// written before the failure are removed, so a failed or cancelled job
/// leaves nothing behind.
#[tracing::instrument(skip_all, fields(frames = request.count))]
pub fn extract_frames(
    request: &ExtractRequest,
    cancel: &AtomicBool,
    done: &AtomicUsize,
) -> Result<ExtractOutcome> {
    let mut reader = Reader::open(&request.video)?;
    let info = reader.info;
    let stem = request.video.file_stem().map_or_else(
        || "Video".to_string(),
        |stem| stem.to_string_lossy().into_owned(),
    );
    let created = !request.folder.exists();
    std::fs::create_dir_all(&request.folder).context("could not create the frames folder")?;

    let mut written = Vec::new();
    let result = (|| -> Result<bool> {
        for (index, time) in frame_times(info.duration, request.count)
            .into_iter()
            .enumerate()
        {
            let _frame = tracing::trace_span!("video.frame", index).entered();
            let Some(frame) = reader.frame_at(time, cancel)? else {
                return Ok(false);
            };
            let image = to_rgb(&frame, &info, request.size)?;
            let path = request
                .folder
                .join(frame_file_name(&stem, index, request.count, time));
            write_jpeg(&image, &path)?;
            written.push(path);
            done.store(written.len(), Ordering::Relaxed);
            if cancel.load(Ordering::Relaxed) {
                return Ok(false);
            }
        }
        Ok(true)
    })();

    match result {
        Ok(true) => Ok(ExtractOutcome::Finished(written)),
        finished => {
            for path in &written {
                let _ = std::fs::remove_file(path);
            }
            if created {
                let _ = std::fs::remove_dir(&request.folder);
            }
            finished.map(|_| ExtractOutcome::Cancelled)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn fixture(name: &str) -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/fixtures")
            .join(name)
    }

    #[test]
    fn frame_times_take_the_middle_of_equal_parts() {
        let times = frame_times(Duration::from_secs(12), 4);
        assert_eq!(
            times,
            [1.5, 4.5, 7.5, 10.5].map(Duration::from_secs_f64).to_vec()
        );
        assert_eq!(frame_times(Duration::from_secs(10), 0).len(), 1);
    }

    #[test]
    fn frame_names_sort_by_time_and_carry_the_timecode() {
        assert_eq!(
            frame_file_name("Walk", 6, 12, Duration::from_millis(72_345)),
            "Walk-007-01m12s345.jpg"
        );
        assert_eq!(
            frame_file_name("Film", 0, 1500, Duration::from_millis(3_723_004)),
            "Film-0001-1h02m03s004.jpg"
        );
    }

    #[test]
    fn display_matrices_become_clockwise_turns() {
        // av_display_rotation_set(-90): what a phone writes for portrait video.
        let fixed = |value: f64| ((value * 65536.0) as i32).to_ne_bytes();
        let matrix: Vec<u8> = [0.0, 1.0, 0.0, -1.0, 0.0, 0.0, 0.0, 0.0, 1.0]
            .into_iter()
            .flat_map(fixed)
            .collect();
        assert_eq!(matrix_turns(&matrix).turns(), 1);
        assert_eq!(matrix_turns(&[]).turns(), 0);
    }

    #[test]
    fn long_edges_scale_proportionally() {
        assert_eq!(FrameSize::LongEdge(1280).fit((1920, 1080)), (1280, 720));
        assert_eq!(FrameSize::LongEdge(4000).fit((1920, 1080)), (1920, 1080));
        assert_eq!(FrameSize::Original.fit((1920, 1080)), (1920, 1080));
    }

    #[test]
    fn probes_length_size_and_rotation() {
        let info = probe(&fixture("clip.mp4")).unwrap();
        assert!((info.duration.as_secs_f64() - 4.0).abs() < 0.05);
        assert_eq!((info.width, info.height), (160, 120));
        assert_eq!(info.frame_rate.map(f64::round), Some(10.0));
        assert_eq!(info.rotation, QuarterTurns::NONE);

        let portrait = probe(&fixture("clip-portrait.mp4")).unwrap();
        assert_eq!(portrait.rotation.turns(), 1);
        assert_eq!(portrait.display_size(), (120, 160));
    }

    #[test]
    fn extracts_upright_frames_and_can_cancel() {
        let folder = tempfile::tempdir().unwrap();
        let request = ExtractRequest {
            video: fixture("clip-portrait.mp4"),
            count: 4,
            size: FrameSize::Original,
            folder: folder.path().join("frames"),
        };
        let done = AtomicUsize::new(0);
        let ExtractOutcome::Finished(frames) =
            extract_frames(&request, &AtomicBool::new(false), &done).unwrap()
        else {
            panic!("the extraction was cancelled");
        };
        assert_eq!(frames.len(), 4);
        assert_eq!(done.load(Ordering::Relaxed), 4);
        let first = image::open(&frames[0]).unwrap();
        assert_eq!((first.width(), first.height()), (120, 160));
        // Frames from different moments of a moving test pattern differ.
        let last = image::open(&frames[3]).unwrap();
        assert_ne!(first.to_rgb8().into_raw(), last.to_rgb8().into_raw());

        let cancelled = ExtractRequest {
            folder: folder.path().join("cancelled"),
            ..request
        };
        let outcome = extract_frames(&cancelled, &AtomicBool::new(true), &done).unwrap();
        assert!(matches!(outcome, ExtractOutcome::Cancelled));
        assert!(!cancelled.folder.exists());
    }

    #[test]
    fn frames_folders_never_reuse_a_full_one() {
        let parent = tempfile::tempdir().unwrap();
        let video = parent.path().join("Walk.mov");
        assert_eq!(frames_folder(&video), parent.path().join("Walk frames"));
        std::fs::create_dir(parent.path().join("Walk frames")).unwrap();
        assert_eq!(frames_folder(&video), parent.path().join("Walk frames"));
        std::fs::write(parent.path().join("Walk frames/a.jpg"), b"x").unwrap();
        assert_eq!(frames_folder(&video), parent.path().join("Walk frames 2"));
    }
}
