//! Non-destructive tone and colour adjustments.
//!
//! Adjustments are stored per photo in the collection and applied to decoded
//! pixels on the way to a preview, a PDF page or a converted copy. The
//! original file is never written.

use std::collections::BTreeMap;
use std::sync::{Arc, OnceLock};

use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::PhotoId;

/// One slider's range, step and neutral value, shared by the UI, the
/// normaliser and the pipeline so they can never disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub min: f32,
    pub max: f32,
    pub step: f32,
}

impl Range {
    const fn new(min: f32, max: f32, step: f32) -> Self {
        Self { min, max, step }
    }

    #[must_use]
    pub fn clamp(self, value: f32) -> f32 {
        // Snap from zero, not from `min`, so neutral values stay exactly 0.0.
        if value.is_finite() {
            ((value / self.step).round() * self.step).clamp(self.min, self.max)
        } else {
            0.0
        }
    }
}

/// The continuous adjustments, in the order the inspector lists them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Slider {
    Exposure,
    Brightness,
    Contrast,
    Shadows,
    Highlights,
    Vibrance,
    Saturation,
    Hue,
    Temperature,
    Tint,
    Threshold,
}

impl Slider {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Exposure => "Exposure",
            Self::Brightness => "Brightness",
            Self::Contrast => "Contrast",
            Self::Shadows => "Shadows",
            Self::Highlights => "Highlights",
            Self::Vibrance => "Vibrance",
            Self::Saturation => "Saturation",
            Self::Hue => "Hue",
            Self::Temperature => "Temperature",
            Self::Tint => "Tint",
            Self::Threshold => "Level",
        }
    }

    #[must_use]
    pub const fn range(self) -> Range {
        match self {
            Self::Exposure => Range::new(-3.0, 3.0, 0.05),
            Self::Hue => Range::new(-180.0, 180.0, 1.0),
            Self::Threshold => Range::new(1.0, 254.0, 1.0),
            _ => Range::new(-100.0, 100.0, 1.0),
        }
    }

    /// The value a slider returns to on reset. Threshold has no neutral: it
    /// only applies while its section is switched on.
    #[must_use]
    pub const fn neutral(self) -> f32 {
        match self {
            Self::Threshold => 128.0,
            _ => 0.0,
        }
    }

    #[must_use]
    pub fn format(self, value: f32) -> String {
        match self {
            Self::Exposure => format!("{value:+.2} EV"),
            Self::Hue => format!("{value:+.0}°"),
            Self::Threshold => format!("{value:.0}"),
            _ => format!("{value:+.0}"),
        }
    }
}

/// Every adjustment one photo carries. The default is the identity: a photo
/// with default adjustments renders exactly as decoded.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Adjustments {
    /// Exposure in stops, applied in linear light.
    pub exposure: f32,
    pub brightness: f32,
    pub contrast: f32,
    pub shadows: f32,
    pub highlights: f32,
    pub vibrance: f32,
    pub saturation: f32,
    /// Hue rotation in degrees.
    pub hue: f32,
    /// Colour balance: negative is cooler, positive warmer.
    pub temperature: f32,
    /// Colour balance: negative is greener, positive more magenta.
    pub tint: f32,
    /// Black and white from Rec. 709 luminance.
    pub monochrome: bool,
    pub invert: bool,
    /// Posterize to this many levels per channel.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub posterize: Option<u8>,
    /// Threshold every pixel to black or white at this luminance level.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub threshold: Option<u8>,
}

pub const POSTERIZE_MIN: u8 = 2;
pub const POSTERIZE_MAX: u8 = 32;
pub const POSTERIZE_DEFAULT: u8 = 6;

impl Default for Adjustments {
    fn default() -> Self {
        Self {
            exposure: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            shadows: 0.0,
            highlights: 0.0,
            vibrance: 0.0,
            saturation: 0.0,
            hue: 0.0,
            temperature: 0.0,
            tint: 0.0,
            monochrome: false,
            invert: false,
            posterize: None,
            threshold: None,
        }
    }
}

impl Adjustments {
    #[must_use]
    pub fn slider(&self, slider: Slider) -> f32 {
        match slider {
            Slider::Exposure => self.exposure,
            Slider::Brightness => self.brightness,
            Slider::Contrast => self.contrast,
            Slider::Shadows => self.shadows,
            Slider::Highlights => self.highlights,
            Slider::Vibrance => self.vibrance,
            Slider::Saturation => self.saturation,
            Slider::Hue => self.hue,
            Slider::Temperature => self.temperature,
            Slider::Tint => self.tint,
            Slider::Threshold => self
                .threshold
                .map_or(Slider::Threshold.neutral(), f32::from),
        }
    }

    pub fn set_slider(&mut self, slider: Slider, value: f32) {
        let value = slider.range().clamp(value);
        match slider {
            Slider::Exposure => self.exposure = value,
            Slider::Brightness => self.brightness = value,
            Slider::Contrast => self.contrast = value,
            Slider::Shadows => self.shadows = value,
            Slider::Highlights => self.highlights = value,
            Slider::Vibrance => self.vibrance = value,
            Slider::Saturation => self.saturation = value,
            Slider::Hue => self.hue = value,
            Slider::Temperature => self.temperature = value,
            Slider::Tint => self.tint = value,
            Slider::Threshold => self.threshold = Some(value as u8),
        }
    }

    /// Clamp every field into its range so a hand-edited collection file can
    /// never produce NaN pixels.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        for slider in [
            Slider::Exposure,
            Slider::Brightness,
            Slider::Contrast,
            Slider::Shadows,
            Slider::Highlights,
            Slider::Vibrance,
            Slider::Saturation,
            Slider::Hue,
            Slider::Temperature,
            Slider::Tint,
        ] {
            self.set_slider(slider, self.slider(slider));
        }
        self.posterize = self
            .posterize
            .map(|levels| levels.clamp(POSTERIZE_MIN, POSTERIZE_MAX));
        self.threshold = self.threshold.map(|level| level.clamp(1, 254));
        self
    }

    #[must_use]
    pub fn is_neutral(&self) -> bool {
        *self == Self::default()
    }

    /// Apply to tightly packed RGBA pixels. Alpha is left untouched.
    pub fn apply_rgba(&self, pixels: &mut [u8]) {
        self.apply(pixels, 4);
    }

    /// Apply to tightly packed RGB pixels.
    pub fn apply_rgb(&self, pixels: &mut [u8]) {
        self.apply(pixels, 3);
    }

    fn apply(&self, pixels: &mut [u8], channels: usize) {
        if self.is_neutral() {
            return;
        }
        let pipeline = Pipeline::new(self);
        pixels.par_chunks_mut(channels * 4096).for_each(|chunk| {
            for pixel in chunk.chunks_exact_mut(channels) {
                let [r, g, b] = pipeline.pixel([pixel[0], pixel[1], pixel[2]]);
                pixel[0] = r;
                pixel[1] = g;
                pixel[2] = b;
            }
        });
    }
}

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// Table resolution for the sRGB encode and tone curves: fine enough that the
/// steep shadow end of the sRGB curve stays within half a level.
const TONE_STEPS: usize = 4096;

fn luma([r, g, b]: [f32; 3]) -> f32 {
    LUMA[2].mul_add(b, LUMA[0].mul_add(r, LUMA[1] * g))
}

fn srgb_to_linear(value: f32) -> f32 {
    if value <= 0.040_45 {
        value / 12.92
    } else {
        ((value + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(value: f32) -> f32 {
    if value <= 0.003_130_8 {
        value * 12.92
    } else {
        1.055f32.mul_add(value.powf(1.0 / 2.4), -0.055)
    }
}

fn decode_table() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| std::array::from_fn(|index| srgb_to_linear(index as f32 / 255.0)))
}

/// Precomputed per-photo constants, so the per-pixel loop is table lookups
/// and a few multiplies.
struct Pipeline {
    /// Linear-light gains from exposure and colour balance.
    gains: [f32; 3],
    /// Linear to sRGB, sampled over 0..=1.
    encode: Vec<f32>,
    /// Brightness then contrast, sampled over 0..=1 in sRGB.
    tone: Vec<f32>,
    shadows: f32,
    highlights: f32,
    saturation: f32,
    vibrance: f32,
    hue: Option<[[f32; 3]; 3]>,
    monochrome: bool,
    invert: bool,
    posterize: Option<f32>,
    threshold: Option<f32>,
}

impl Pipeline {
    fn new(adjustments: &Adjustments) -> Self {
        let exposure = adjustments.exposure.exp2();
        let warm = adjustments.temperature / 100.0;
        let magenta = adjustments.tint / 100.0;
        let gains = [
            exposure * 0.25f32.mul_add(warm, 1.0),
            exposure * (-0.2f32).mul_add(magenta, 1.0),
            exposure * (-0.25f32).mul_add(warm, 1.0),
        ];

        let encode = (0..=TONE_STEPS)
            .map(|step| linear_to_srgb(step as f32 / TONE_STEPS as f32))
            .collect();

        // Brightness is a gamma curve so black and white stay put; contrast is
        // a symmetric power curve around mid-grey.
        let gamma = (-adjustments.brightness / 100.0).exp2();
        let power = (adjustments.contrast / 100.0 * 1.5).exp2();
        let tone = (0..=TONE_STEPS)
            .map(|step| {
                let value = (step as f32 / TONE_STEPS as f32).powf(gamma);
                if value < 0.5 {
                    0.5 * (2.0 * value).powf(power)
                } else {
                    0.5f32.mul_add(-2.0f32.mul_add(-value, 2.0).powf(power), 1.0)
                }
            })
            .collect();

        let hue = (adjustments.hue != 0.0).then(|| hue_matrix(adjustments.hue.to_radians()));
        Self {
            gains,
            encode,
            tone,
            shadows: adjustments.shadows / 100.0,
            highlights: adjustments.highlights / 100.0,
            saturation: adjustments.saturation / 100.0,
            vibrance: adjustments.vibrance / 100.0,
            hue,
            monochrome: adjustments.monochrome,
            invert: adjustments.invert,
            posterize: adjustments
                .posterize
                .map(|levels| f32::from(levels.max(POSTERIZE_MIN) - 1)),
            threshold: adjustments.threshold.map(|level| f32::from(level) / 255.0),
        }
    }

    fn sample(table: &[f32], value: f32) -> f32 {
        let index = (value.clamp(0.0, 1.0) * TONE_STEPS as f32).round() as usize;
        table[index.min(TONE_STEPS)]
    }

    fn pixel(&self, [r, g, b]: [u8; 3]) -> [u8; 3] {
        let decode = decode_table();
        let mut rgb = [
            Self::sample(&self.encode, decode[usize::from(r)] * self.gains[0]),
            Self::sample(&self.encode, decode[usize::from(g)] * self.gains[1]),
            Self::sample(&self.encode, decode[usize::from(b)] * self.gains[2]),
        ];

        if self.shadows != 0.0 || self.highlights != 0.0 {
            let y = luma(rgb);
            if y > 1e-4 {
                // Lift or deepen the darker and lighter halves of the tonal
                // range; black and white stay fixed so nothing clips.
                let lifted = (self.highlights * 1.5 * y * y).mul_add(
                    1.0 - y,
                    (self.shadows * 1.5 * y * (1.0 - y)).mul_add(1.0 - y, y),
                );
                let scale = lifted.clamp(0.0, 1.0) / y;
                rgb = rgb.map(|channel| channel * scale);
            }
        }

        rgb = rgb.map(|channel| Self::sample(&self.tone, channel));

        if self.saturation != 0.0 || self.vibrance != 0.0 {
            let y = luma(rgb);
            let chroma = rgb[0].max(rgb[1]).max(rgb[2]) - rgb[0].min(rgb[1]).min(rgb[2]);
            // Vibrance favours muted colours so skin and skies saturate last.
            let factor = self
                .vibrance
                .mul_add(1.0 - chroma, 1.0 + self.saturation)
                .max(0.0);
            rgb = rgb.map(|channel| (channel - y).mul_add(factor, y));
        }

        if let Some(matrix) = &self.hue {
            rgb = matrix.map(|row| row[2].mul_add(rgb[2], row[0].mul_add(rgb[0], row[1] * rgb[1])));
        }

        if self.monochrome {
            rgb = [luma(rgb); 3];
        }
        if self.invert {
            rgb = rgb.map(|channel| 1.0 - channel);
        }
        if let Some(steps) = self.posterize {
            rgb = rgb.map(|channel| (channel.clamp(0.0, 1.0) * steps).round() / steps);
        }
        if let Some(level) = self.threshold {
            rgb = [if luma(rgb) >= level { 1.0 } else { 0.0 }; 3];
        }

        rgb.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as u8)
    }
}

/// Rotate hue around the grey axis while keeping luminance, as CSS
/// `hue-rotate` does: each coefficient is `base + cos·c + sin·s`.
fn hue_matrix(angle: f32) -> [[f32; 3]; 3] {
    const BASE: [f32; 3] = [0.213, 0.715, 0.072];
    const COS: [[f32; 3]; 3] = [
        [0.787, -0.715, -0.072],
        [-0.213, 0.285, -0.072],
        [-0.213, -0.715, 0.928],
    ];
    const SIN: [[f32; 3]; 3] = [
        [-0.213, -0.715, 0.928],
        [0.143, 0.140, -0.283],
        [-0.787, 0.715, 0.072],
    ];
    let (sin, cos) = angle.sin_cos();
    std::array::from_fn(|row| {
        std::array::from_fn(|column| {
            SIN[row][column].mul_add(sin, COS[row][column].mul_add(cos, BASE[column]))
        })
    })
}

/// The adjusted photos of a collection. Most photos carry none, so this is a
/// sparse map; it sits behind an `Arc` so whole-collection undo snapshots
/// share it instead of copying it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AdjustmentMap(Arc<BTreeMap<PhotoId, Adjustments>>);

impl AdjustmentMap {
    #[must_use]
    pub fn get(&self, id: PhotoId) -> Option<&Adjustments> {
        self.0.get(&id)
    }

    /// The photo's adjustments, or the identity when it has none.
    #[must_use]
    pub fn for_photo(&self, id: PhotoId) -> Adjustments {
        self.get(id).copied().unwrap_or_default()
    }

    /// Store `adjustments` for `id`; neutral adjustments remove the entry so
    /// the collection file only lists photos that are actually adjusted.
    pub fn set(&mut self, id: PhotoId, adjustments: Adjustments) {
        let adjustments = adjustments.normalized();
        if adjustments.is_neutral() {
            if self.0.contains_key(&id) {
                Arc::make_mut(&mut self.0).remove(&id);
            }
        } else if self.0.get(&id) != Some(&adjustments) {
            Arc::make_mut(&mut self.0).insert(id, adjustments);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(PhotoId) -> bool) {
        if self.0.keys().any(|id| !keep(*id)) {
            Arc::make_mut(&mut self.0).retain(|id, _| keep(*id));
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.0.len()
    }
}

impl Serialize for AdjustmentMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AdjustmentMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let map = BTreeMap::<PhotoId, Adjustments>::deserialize(deserializer)?;
        Ok(Self(Arc::new(
            map.into_iter()
                .map(|(id, adjustments)| (id, adjustments.normalized()))
                .filter(|(_, adjustments)| !adjustments.is_neutral())
                .collect(),
        )))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn rgb(adjustments: &Adjustments, pixel: [u8; 3]) -> [u8; 3] {
        let mut pixels = pixel.to_vec();
        adjustments.apply_rgb(&mut pixels);
        [pixels[0], pixels[1], pixels[2]]
    }

    #[test]
    fn neutral_adjustments_leave_pixels_untouched() {
        let adjustments = Adjustments {
            exposure: 0.0,
            ..Adjustments::default()
        };
        assert!(adjustments.is_neutral());
        for pixel in [[0, 0, 0], [12, 128, 250], [255, 255, 255]] {
            assert_eq!(rgb(&adjustments, pixel), pixel);
        }
    }

    #[test]
    fn a_non_neutral_pipeline_round_trips_mid_tones() {
        // Saturation +0 with a tiny hue turn exercises every stage but should
        // stay within a step of the input.
        let adjustments = Adjustments {
            hue: 0.0,
            shadows: 0.0,
            temperature: 0.0,
            brightness: 0.0,
            contrast: 0.0,
            saturation: 1.0,
            ..Adjustments::default()
        };
        let [r, g, b] = rgb(&adjustments, [90, 140, 200]);
        assert!(r.abs_diff(90) <= 2 && g.abs_diff(140) <= 2 && b.abs_diff(200) <= 2);
    }

    #[test]
    fn exposure_brightens_and_darkens() {
        let brighter = Adjustments {
            exposure: 1.0,
            ..Adjustments::default()
        };
        let darker = Adjustments {
            exposure: -1.0,
            ..Adjustments::default()
        };
        assert!(rgb(&brighter, [100, 100, 100])[0] > 100);
        assert!(rgb(&darker, [100, 100, 100])[0] < 100);
        assert_eq!(rgb(&brighter, [0, 0, 0]), [0, 0, 0]);
    }

    #[test]
    fn monochrome_invert_and_threshold_behave() {
        let mono = Adjustments {
            monochrome: true,
            ..Adjustments::default()
        };
        let [r, g, b] = rgb(&mono, [200, 40, 40]);
        assert!(r == g && g == b);

        let invert = Adjustments {
            invert: true,
            ..Adjustments::default()
        };
        assert_eq!(rgb(&invert, [0, 255, 10]), [255, 0, 245]);

        let threshold = Adjustments {
            threshold: Some(128),
            ..Adjustments::default()
        };
        assert_eq!(rgb(&threshold, [30, 30, 30]), [0, 0, 0]);
        assert_eq!(rgb(&threshold, [220, 220, 220]), [255, 255, 255]);
    }

    #[test]
    fn contrast_and_brightness_keep_black_and_white() {
        let adjustments = Adjustments {
            contrast: 80.0,
            brightness: 60.0,
            ..Adjustments::default()
        };
        assert_eq!(rgb(&adjustments, [0, 0, 0]), [0, 0, 0]);
        assert_eq!(rgb(&adjustments, [255, 255, 255]), [255, 255, 255]);
    }

    #[test]
    fn normalising_clamps_hand_edited_values() {
        let adjustments = Adjustments {
            exposure: 9.0,
            contrast: f32::NAN,
            posterize: Some(200),
            ..Adjustments::default()
        }
        .normalized();
        assert!((adjustments.exposure - 3.0).abs() < f32::EPSILON);
        assert!(adjustments.contrast.abs() < f32::EPSILON);
        assert_eq!(adjustments.posterize, Some(POSTERIZE_MAX));
    }

    #[test]
    fn the_map_drops_neutral_entries_and_round_trips() {
        let mut map = AdjustmentMap::default();
        map.set(
            PhotoId(3),
            Adjustments {
                exposure: 0.5,
                monochrome: true,
                ..Adjustments::default()
            },
        );
        map.set(PhotoId(4), Adjustments::default());
        assert_eq!(map.len(), 1);

        let json = serde_json::to_string(&map).unwrap();
        let back: AdjustmentMap = serde_json::from_str(&json).unwrap();
        assert_eq!(back, map);

        map.set(PhotoId(3), Adjustments::default());
        assert!(map.is_empty());
    }

    #[test]
    fn snapshots_share_the_map_until_one_changes() {
        let mut map = AdjustmentMap::default();
        map.set(
            PhotoId(1),
            Adjustments {
                saturation: 20.0,
                ..Adjustments::default()
            },
        );
        let snapshot = map.clone();
        assert!(Arc::ptr_eq(&map.0, &snapshot.0));
        map.set(
            PhotoId(2),
            Adjustments {
                invert: true,
                ..Adjustments::default()
            },
        );
        assert!(!Arc::ptr_eq(&map.0, &snapshot.0));
        assert_eq!(snapshot.len(), 1);
    }
}
