//! Non-destructive tone and colour adjustments.
//!
//! Adjustments are stored per photo in the collection and applied to decoded
//! pixels on the way to a preview, a PDF page or a converted copy. The
//! original file is never written.

use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};

use anyhow::{Context as _, Result, bail};
use rayon::prelude::*;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::model::PhotoId;

/// How a slider's track maps to its value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scale {
    Linear,
    /// Equal track distances multiply the value, as gamma needs.
    Log,
}

/// One slider's range and step, shared by the UI, the normaliser and the
/// pipeline so they can never disagree.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Range {
    pub min: f32,
    pub max: f32,
    pub step: f32,
    pub scale: Scale,
}

impl Range {
    const fn linear(min: f32, max: f32, step: f32) -> Self {
        Self {
            min,
            max,
            step,
            scale: Scale::Linear,
        }
    }

    #[must_use]
    pub fn clamp(self, value: f32) -> f32 {
        // Snap from zero, not from `min`, so neutral values stay exact.
        if value.is_finite() {
            ((value / self.step).round() * self.step).clamp(self.min, self.max)
        } else {
            self.min.max(0.0).min(self.max)
        }
    }

    /// Where `value` sits along the track, from 0 to 1.
    #[must_use]
    pub fn fraction(self, value: f32) -> f32 {
        let fraction = match self.scale {
            Scale::Linear => (value - self.min) / (self.max - self.min),
            Scale::Log => {
                (value.max(self.min).ln() - self.min.ln()) / (self.max.ln() - self.min.ln())
            }
        };
        fraction.clamp(0.0, 1.0)
    }

    /// The value at a point along the track.
    #[must_use]
    pub fn value_at(self, fraction: f32) -> f32 {
        let fraction = fraction.clamp(0.0, 1.0);
        let value = match self.scale {
            Scale::Linear => fraction.mul_add(self.max - self.min, self.min),
            Scale::Log => fraction
                .mul_add(self.max.ln() - self.min.ln(), self.min.ln())
                .exp(),
        };
        self.clamp(value)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Channel {
    Red,
    Green,
    Blue,
}

impl Channel {
    pub const ALL: [Self; 3] = [Self::Red, Self::Green, Self::Blue];

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Red => 0,
            Self::Green => 1,
            Self::Blue => 2,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Red => "Red",
            Self::Green => "Green",
            Self::Blue => "Blue",
        }
    }
}

/// The six hue families selective colour adjusts, 60° apart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum HueRange {
    Reds,
    Yellows,
    Greens,
    Cyans,
    Blues,
    Magentas,
}

impl HueRange {
    pub const ALL: [Self; 6] = [
        Self::Reds,
        Self::Yellows,
        Self::Greens,
        Self::Cyans,
        Self::Blues,
        Self::Magentas,
    ];

    #[must_use]
    pub const fn index(self) -> usize {
        match self {
            Self::Reds => 0,
            Self::Yellows => 1,
            Self::Greens => 2,
            Self::Cyans => 3,
            Self::Blues => 4,
            Self::Magentas => 5,
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Reds => "Reds",
            Self::Yellows => "Yellows",
            Self::Greens => "Greens",
            Self::Cyans => "Cyans",
            Self::Blues => "Blues",
            Self::Magentas => "Magentas",
        }
    }

    #[must_use]
    pub fn centre(self) -> f32 {
        self.index() as f32 * 60.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LevelsKnob {
    InputBlack,
    Gamma,
    InputWhite,
    OutputBlack,
    OutputWhite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FilterKnob {
    Colour,
    Density,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GradientKnob {
    ShadowHue,
    ShadowSaturation,
    HighlightHue,
    HighlightSaturation,
    Amount,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ToneKnob {
    Hue,
    Saturation,
    Lightness,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ReplaceKnob {
    Colour,
    Range,
    Tone(ToneKnob),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum MatchKnob {
    Luminance,
    Intensity,
    Fade,
}

/// Every continuous control. Each knows its range, neutral value, label and
/// display format, and reads and writes one field of [`Adjustments`].
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
    Levels(LevelsKnob),
    /// One of the five curve points, at inputs 0, 64, 128, 191 and 255.
    Curve(u8),
    Mix {
        output: Channel,
        input: Channel,
    },
    Filter(FilterKnob),
    Gradient(GradientKnob),
    Selective(HueRange, ToneKnob),
    Replace(ReplaceKnob),
    Match(MatchKnob),
    LookupAmount,
}

pub const CURVE_POINTS: usize = 5;

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
            Self::Saturation
            | Self::Selective(_, ToneKnob::Saturation)
            | Self::Replace(ReplaceKnob::Tone(ToneKnob::Saturation)) => "Saturation",
            Self::Hue | Self::Selective(_, ToneKnob::Hue) => "Hue",
            Self::Temperature => "Temperature",
            Self::Tint => "Tint",
            Self::Threshold => "Level",
            Self::Levels(LevelsKnob::InputBlack) => "Input black",
            Self::Levels(LevelsKnob::Gamma) => "Midtones",
            Self::Levels(LevelsKnob::InputWhite) => "Input white",
            Self::Levels(LevelsKnob::OutputBlack) => "Output black",
            Self::Levels(LevelsKnob::OutputWhite) => "Output white",
            Self::Curve(_) => "Point",
            Self::Mix { input, .. } => input.label(),
            Self::Filter(FilterKnob::Colour) | Self::Replace(ReplaceKnob::Colour) => "Colour",
            Self::Filter(FilterKnob::Density) => "Density",
            Self::Gradient(GradientKnob::ShadowHue) => "Shadow hue",
            Self::Gradient(GradientKnob::ShadowSaturation) => "Shadow saturation",
            Self::Gradient(GradientKnob::HighlightHue) => "Highlight hue",
            Self::Gradient(GradientKnob::HighlightSaturation) => "Highlight saturation",
            Self::Gradient(GradientKnob::Amount) | Self::LookupAmount => "Amount",
            Self::Selective(_, ToneKnob::Lightness)
            | Self::Replace(ReplaceKnob::Tone(ToneKnob::Lightness)) => "Lightness",
            Self::Replace(ReplaceKnob::Range) => "Range",
            Self::Replace(ReplaceKnob::Tone(ToneKnob::Hue)) => "Hue shift",
            Self::Match(MatchKnob::Luminance) => "Luminance",
            Self::Match(MatchKnob::Intensity) => "Colour intensity",
            Self::Match(MatchKnob::Fade) => "Fade",
        }
    }

    #[must_use]
    pub const fn range(self) -> Range {
        match self {
            Self::Exposure => Range::linear(-3.0, 3.0, 0.05),
            Self::Hue | Self::Replace(ReplaceKnob::Tone(ToneKnob::Hue)) => {
                Range::linear(-180.0, 180.0, 1.0)
            }
            Self::Threshold => Range::linear(1.0, 254.0, 1.0),
            Self::Levels(LevelsKnob::Gamma) => Range {
                min: 0.1,
                max: 9.99,
                step: 0.01,
                scale: Scale::Log,
            },
            Self::Levels(LevelsKnob::InputBlack) => Range::linear(0.0, 253.0, 1.0),
            Self::Levels(LevelsKnob::InputWhite) => Range::linear(2.0, 255.0, 1.0),
            Self::Levels(LevelsKnob::OutputBlack | LevelsKnob::OutputWhite) | Self::Curve(_) => {
                Range::linear(0.0, 255.0, 1.0)
            }
            Self::Mix { .. } => Range::linear(-200.0, 200.0, 1.0),
            Self::Filter(FilterKnob::Colour)
            | Self::Gradient(GradientKnob::ShadowHue | GradientKnob::HighlightHue)
            | Self::Replace(ReplaceKnob::Colour) => Range::linear(0.0, 359.0, 1.0),
            Self::Filter(FilterKnob::Density)
            | Self::Gradient(
                GradientKnob::ShadowSaturation
                | GradientKnob::HighlightSaturation
                | GradientKnob::Amount,
            )
            | Self::Match(MatchKnob::Fade)
            | Self::LookupAmount => Range::linear(0.0, 100.0, 1.0),
            Self::Replace(ReplaceKnob::Range) => Range::linear(5.0, 90.0, 1.0),
            Self::Match(MatchKnob::Luminance) => Range::linear(1.0, 200.0, 1.0),
            Self::Match(MatchKnob::Intensity) => Range::linear(0.0, 200.0, 1.0),
            _ => Range::linear(-100.0, 100.0, 1.0),
        }
    }

    /// The value a reset returns to: the identity for always-on controls, the
    /// starting value for controls inside a switchable group.
    #[must_use]
    pub const fn neutral(self) -> f32 {
        match self {
            Self::Threshold | Self::Curve(2) => 128.0,
            Self::Levels(LevelsKnob::Gamma) => 1.0,
            Self::Levels(LevelsKnob::InputWhite | LevelsKnob::OutputWhite) | Self::Curve(4) => {
                255.0
            }
            Self::Curve(1) => 64.0,
            Self::Curve(3) => 191.0,
            Self::Mix { output, input } => {
                if output.index() == input.index() {
                    100.0
                } else {
                    0.0
                }
            }
            Self::Filter(FilterKnob::Colour) | Self::Replace(ReplaceKnob::Range) => 30.0,
            Self::Filter(FilterKnob::Density) => 25.0,
            Self::Gradient(GradientKnob::ShadowHue) => 220.0,
            Self::Gradient(GradientKnob::HighlightHue) => 40.0,
            Self::Gradient(GradientKnob::ShadowSaturation | GradientKnob::HighlightSaturation) => {
                50.0
            }
            Self::Gradient(GradientKnob::Amount)
            | Self::LookupAmount
            | Self::Match(MatchKnob::Luminance | MatchKnob::Intensity) => 100.0,
            _ => 0.0,
        }
    }

    /// Hue controls pick a colour, so their track shows the hue spectrum.
    #[must_use]
    pub const fn is_hue_picker(self) -> bool {
        matches!(
            self,
            Self::Filter(FilterKnob::Colour)
                | Self::Gradient(GradientKnob::ShadowHue | GradientKnob::HighlightHue)
                | Self::Replace(ReplaceKnob::Colour)
        )
    }

    #[must_use]
    pub fn format(self, value: f32) -> String {
        match self {
            Self::Exposure => format!("{value:+.2} EV"),
            Self::Hue | Self::Replace(ReplaceKnob::Tone(ToneKnob::Hue)) => {
                format!("{value:+.0}°")
            }
            Self::Levels(LevelsKnob::Gamma) => format!("{value:.2}"),
            Self::Filter(FilterKnob::Colour)
            | Self::Gradient(GradientKnob::ShadowHue | GradientKnob::HighlightHue)
            | Self::Replace(ReplaceKnob::Colour | ReplaceKnob::Range) => format!("{value:.0}°"),
            Self::Mix { .. }
            | Self::Filter(FilterKnob::Density)
            | Self::Gradient(
                GradientKnob::ShadowSaturation
                | GradientKnob::HighlightSaturation
                | GradientKnob::Amount,
            )
            | Self::Match(_)
            | Self::LookupAmount => format!("{value:.0}%"),
            Self::Threshold | Self::Levels(_) | Self::Curve(_) => format!("{value:.0}"),
            _ => format!("{value:+.0}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Levels {
    pub input_black: f32,
    pub gamma: f32,
    pub input_white: f32,
    pub output_black: f32,
    pub output_white: f32,
}

impl Default for Levels {
    fn default() -> Self {
        Self {
            input_black: 0.0,
            gamma: 1.0,
            input_white: 255.0,
            output_black: 0.0,
            output_white: 255.0,
        }
    }
}

/// A monotone tone curve through five points at fixed inputs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Curve {
    pub points: [f32; CURVE_POINTS],
}

impl Curve {
    pub const INPUTS: [f32; CURVE_POINTS] = [0.0, 64.0, 128.0, 191.0, 255.0];
}

impl Default for Curve {
    fn default() -> Self {
        Self {
            points: Self::INPUTS,
        }
    }
}

/// Each output channel as a percentage mix of the three inputs.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ChannelMixer {
    pub rows: [[f32; 3]; 3],
}

impl Default for ChannelMixer {
    fn default() -> Self {
        Self {
            rows: [[100.0, 0.0, 0.0], [0.0, 100.0, 0.0], [0.0, 0.0, 100.0]],
        }
    }
}

/// A coloured lens filter that keeps the photo's luminosity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PhotoFilter {
    pub hue: f32,
    pub density: f32,
}

impl Default for PhotoFilter {
    fn default() -> Self {
        Self {
            hue: Slider::Filter(FilterKnob::Colour).neutral(),
            density: Slider::Filter(FilterKnob::Density).neutral(),
        }
    }
}

/// Map luminance onto a two-colour gradient, from the shadow colour to the
/// highlight colour, keeping each pixel's tone.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GradientMap {
    pub shadow_hue: f32,
    pub shadow_saturation: f32,
    pub highlight_hue: f32,
    pub highlight_saturation: f32,
    pub amount: f32,
}

impl GradientMap {
    /// The colour a pixel of luminance `y` (0 to 1) maps to, before `amount`.
    #[must_use]
    pub fn colour_at(&self, y: f32) -> [f32; 3] {
        gradient_tint(
            hue_colour(self.shadow_hue, self.shadow_saturation / 100.0),
            hue_colour(self.highlight_hue, self.highlight_saturation / 100.0),
            y,
        )
    }
}

/// Blend the two gradient colours by luminance, then rescale so the result
/// keeps that luminance.
fn gradient_tint(shadow: [f32; 3], highlight: [f32; 3], y: f32) -> [f32; 3] {
    let y = y.clamp(0.0, 1.0);
    let tint = lerp(shadow, highlight, y);
    let scale = y / luma(tint).max(1e-4);
    tint.map(|channel| (channel * scale).clamp(0.0, 1.0))
}

impl Default for GradientMap {
    fn default() -> Self {
        Self {
            shadow_hue: Slider::Gradient(GradientKnob::ShadowHue).neutral(),
            shadow_saturation: Slider::Gradient(GradientKnob::ShadowSaturation).neutral(),
            highlight_hue: Slider::Gradient(GradientKnob::HighlightHue).neutral(),
            highlight_saturation: Slider::Gradient(GradientKnob::HighlightSaturation).neutral(),
            amount: Slider::Gradient(GradientKnob::Amount).neutral(),
        }
    }
}

/// Hue, saturation and lightness shifts for each hue family.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct SelectiveColour {
    pub ranges: [[f32; 3]; 6],
}

/// Shift the hue, saturation and lightness of one chosen colour.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ReplaceColour {
    pub colour: f32,
    pub range: f32,
    pub hue: f32,
    pub saturation: f32,
    pub lightness: f32,
}

impl Default for ReplaceColour {
    fn default() -> Self {
        Self {
            colour: 0.0,
            range: Slider::Replace(ReplaceKnob::Range).neutral(),
            hue: 0.0,
            saturation: 0.0,
            lightness: 0.0,
        }
    }
}

/// Mean and spread of a photo's luma and chroma, in full-range `YCbCr`.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub struct ColourStats {
    pub mean: [f32; 3],
    pub deviation: [f32; 3],
}

/// Transfer another photo's colour statistics onto this one.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct MatchColour {
    pub source: PhotoId,
    pub target: ColourStats,
    #[serde(default = "full")]
    pub luminance: f32,
    #[serde(default = "full")]
    pub intensity: f32,
    #[serde(default)]
    pub fade: f32,
}

const fn full() -> f32 {
    100.0
}

impl MatchColour {
    #[must_use]
    pub const fn new(source: PhotoId, target: ColourStats) -> Self {
        Self {
            source,
            target,
            luminance: 100.0,
            intensity: 100.0,
            fade: 0.0,
        }
    }
}

/// A 3D colour lookup table loaded from a `.cube` file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct LutId(pub u64);

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ColourLookup {
    pub lut: LutId,
    #[serde(default = "full")]
    pub amount: f32,
}

/// Every adjustment one photo carries.
///
/// The default is the identity: a photo with default adjustments renders
/// exactly as decoded. A switchable group is `None` while off; its sliders
/// only edit it while it is on.
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
    #[serde(skip_serializing_if = "is_default")]
    pub levels: Levels,
    #[serde(skip_serializing_if = "is_default")]
    pub curve: Curve,
    #[serde(skip_serializing_if = "is_default")]
    pub mixer: ChannelMixer,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub photo_filter: Option<PhotoFilter>,
    #[serde(skip_serializing_if = "is_default")]
    pub selective: SelectiveColour,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub replace: Option<ReplaceColour>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub match_colour: Option<MatchColour>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gradient_map: Option<GradientMap>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lookup: Option<ColourLookup>,
}

fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
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
            levels: Levels::default(),
            curve: Curve::default(),
            mixer: ChannelMixer::default(),
            photo_filter: None,
            selective: SelectiveColour::default(),
            replace: None,
            match_colour: None,
            gradient_map: None,
            lookup: None,
        }
    }
}

impl Adjustments {
    /// The current value of a control. A control inside a switched-off group
    /// reads as its starting value.
    #[must_use]
    pub fn slider(&self, slider: Slider) -> f32 {
        let neutral = slider.neutral();
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
            Slider::Threshold => self.threshold.map_or(neutral, f32::from),
            Slider::Levels(knob) => match knob {
                LevelsKnob::InputBlack => self.levels.input_black,
                LevelsKnob::Gamma => self.levels.gamma,
                LevelsKnob::InputWhite => self.levels.input_white,
                LevelsKnob::OutputBlack => self.levels.output_black,
                LevelsKnob::OutputWhite => self.levels.output_white,
            },
            Slider::Curve(point) => self
                .curve
                .points
                .get(usize::from(point))
                .copied()
                .unwrap_or(neutral),
            Slider::Mix { output, input } => self.mixer.rows[output.index()][input.index()],
            Slider::Filter(knob) => self.photo_filter.map_or(neutral, |filter| match knob {
                FilterKnob::Colour => filter.hue,
                FilterKnob::Density => filter.density,
            }),
            Slider::Gradient(knob) => self.gradient_map.map_or(neutral, |map| match knob {
                GradientKnob::ShadowHue => map.shadow_hue,
                GradientKnob::ShadowSaturation => map.shadow_saturation,
                GradientKnob::HighlightHue => map.highlight_hue,
                GradientKnob::HighlightSaturation => map.highlight_saturation,
                GradientKnob::Amount => map.amount,
            }),
            Slider::Selective(range, knob) => self.selective.ranges[range.index()][tone(knob)],
            Slider::Replace(knob) => self.replace.map_or(neutral, |replace| match knob {
                ReplaceKnob::Colour => replace.colour,
                ReplaceKnob::Range => replace.range,
                ReplaceKnob::Tone(ToneKnob::Hue) => replace.hue,
                ReplaceKnob::Tone(ToneKnob::Saturation) => replace.saturation,
                ReplaceKnob::Tone(ToneKnob::Lightness) => replace.lightness,
            }),
            Slider::Match(knob) => self.match_colour.map_or(neutral, |matched| match knob {
                MatchKnob::Luminance => matched.luminance,
                MatchKnob::Intensity => matched.intensity,
                MatchKnob::Fade => matched.fade,
            }),
            Slider::LookupAmount => self.lookup.map_or(neutral, |lookup| lookup.amount),
        }
    }

    /// Set a control, clamped to its range. Controls inside a switched-off
    /// group do nothing: the group's toggle turns it on.
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
            Slider::Threshold => {
                if let Some(level) = &mut self.threshold {
                    *level = value as u8;
                }
            }
            Slider::Levels(knob) => {
                let levels = &mut self.levels;
                match knob {
                    LevelsKnob::InputBlack => {
                        levels.input_black = value.min(levels.input_white - 2.0);
                    }
                    LevelsKnob::Gamma => levels.gamma = value,
                    LevelsKnob::InputWhite => {
                        levels.input_white = value.max(levels.input_black + 2.0);
                    }
                    LevelsKnob::OutputBlack => levels.output_black = value,
                    LevelsKnob::OutputWhite => levels.output_white = value,
                }
            }
            Slider::Curve(point) => {
                if let Some(output) = self.curve.points.get_mut(usize::from(point)) {
                    *output = value;
                }
            }
            Slider::Mix { output, input } => {
                self.mixer.rows[output.index()][input.index()] = value;
            }
            Slider::Filter(knob) => {
                if let Some(filter) = &mut self.photo_filter {
                    match knob {
                        FilterKnob::Colour => filter.hue = value,
                        FilterKnob::Density => filter.density = value,
                    }
                }
            }
            Slider::Gradient(knob) => {
                if let Some(map) = &mut self.gradient_map {
                    match knob {
                        GradientKnob::ShadowHue => map.shadow_hue = value,
                        GradientKnob::ShadowSaturation => map.shadow_saturation = value,
                        GradientKnob::HighlightHue => map.highlight_hue = value,
                        GradientKnob::HighlightSaturation => map.highlight_saturation = value,
                        GradientKnob::Amount => map.amount = value,
                    }
                }
            }
            Slider::Selective(range, knob) => {
                self.selective.ranges[range.index()][tone(knob)] = value;
            }
            Slider::Replace(knob) => {
                if let Some(replace) = &mut self.replace {
                    match knob {
                        ReplaceKnob::Colour => replace.colour = value,
                        ReplaceKnob::Range => replace.range = value,
                        ReplaceKnob::Tone(ToneKnob::Hue) => replace.hue = value,
                        ReplaceKnob::Tone(ToneKnob::Saturation) => replace.saturation = value,
                        ReplaceKnob::Tone(ToneKnob::Lightness) => replace.lightness = value,
                    }
                }
            }
            Slider::Match(knob) => {
                if let Some(matched) = &mut self.match_colour {
                    match knob {
                        MatchKnob::Luminance => matched.luminance = value,
                        MatchKnob::Intensity => matched.intensity = value,
                        MatchKnob::Fade => matched.fade = value,
                    }
                }
            }
            Slider::LookupAmount => {
                if let Some(lookup) = &mut self.lookup {
                    lookup.amount = value;
                }
            }
        }
    }

    /// Clamp every field into its range so a hand-edited collection file can
    /// never produce NaN pixels.
    #[must_use]
    pub fn normalized(mut self) -> Self {
        for slider in Slider::every() {
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
        let mut pipeline = Pipeline::new(self);
        if let Some(matched) = &mut pipeline.matched {
            matched.source = colour_stats(pixels, channels);
        }
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

const fn tone(knob: ToneKnob) -> usize {
    match knob {
        ToneKnob::Hue => 0,
        ToneKnob::Saturation => 1,
        ToneKnob::Lightness => 2,
    }
}

impl Slider {
    /// Every control, for normalising a whole adjustment set.
    fn every() -> Vec<Self> {
        let mut sliders = vec![
            Self::Exposure,
            Self::Brightness,
            Self::Contrast,
            Self::Shadows,
            Self::Highlights,
            Self::Vibrance,
            Self::Saturation,
            Self::Hue,
            Self::Temperature,
            Self::Tint,
            Self::Levels(LevelsKnob::InputBlack),
            Self::Levels(LevelsKnob::InputWhite),
            Self::Levels(LevelsKnob::Gamma),
            Self::Levels(LevelsKnob::OutputBlack),
            Self::Levels(LevelsKnob::OutputWhite),
            Self::Filter(FilterKnob::Colour),
            Self::Filter(FilterKnob::Density),
            Self::Gradient(GradientKnob::ShadowHue),
            Self::Gradient(GradientKnob::ShadowSaturation),
            Self::Gradient(GradientKnob::HighlightHue),
            Self::Gradient(GradientKnob::HighlightSaturation),
            Self::Gradient(GradientKnob::Amount),
            Self::Replace(ReplaceKnob::Colour),
            Self::Replace(ReplaceKnob::Range),
            Self::Match(MatchKnob::Luminance),
            Self::Match(MatchKnob::Intensity),
            Self::Match(MatchKnob::Fade),
            Self::LookupAmount,
        ];
        sliders.extend((0..CURVE_POINTS as u8).map(Self::Curve));
        for output in Channel::ALL {
            sliders.extend(Channel::ALL.map(|input| Self::Mix { output, input }));
        }
        for knob in [ToneKnob::Hue, ToneKnob::Saturation, ToneKnob::Lightness] {
            sliders.extend(HueRange::ALL.map(|range| Self::Selective(range, knob)));
            sliders.push(Self::Replace(ReplaceKnob::Tone(knob)));
        }
        sliders
    }
}

// ----- colour maths --------------------------------------------------------

const LUMA: [f32; 3] = [0.2126, 0.7152, 0.0722];
/// Table resolution for the sRGB encode and tone curves: fine enough that the
/// steep shadow end of the sRGB curve stays within half a level.
const TONE_STEPS: usize = 4096;

fn luma([r, g, b]: [f32; 3]) -> f32 {
    LUMA[2].mul_add(b, LUMA[0].mul_add(r, LUMA[1] * g))
}

fn lerp(from: [f32; 3], to: [f32; 3], amount: f32) -> [f32; 3] {
    std::array::from_fn(|index| (to[index] - from[index]).mul_add(amount, from[index]))
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

/// Hue in degrees, saturation and lightness, all from 0 to 1 except hue.
fn rgb_to_hsl([r, g, b]: [f32; 3]) -> (f32, f32, f32) {
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);
    let lightness = f32::midpoint(max, min);
    let delta = max - min;
    if delta < 1e-6 {
        return (0.0, 0.0, lightness);
    }
    let saturation = delta / (1.0 - 2.0f32.mul_add(lightness, -1.0).abs()).max(1e-6);
    let hue = if (max - r).abs() < f32::EPSILON {
        ((g - b) / delta).rem_euclid(6.0)
    } else if (max - g).abs() < f32::EPSILON {
        (b - r) / delta + 2.0
    } else {
        (r - g) / delta + 4.0
    };
    (hue * 60.0, saturation.min(1.0), lightness)
}

fn hsl_to_rgb(hue: f32, saturation: f32, lightness: f32) -> [f32; 3] {
    let chroma = (1.0 - 2.0f32.mul_add(lightness, -1.0).abs()) * saturation;
    let sector = hue.rem_euclid(360.0) / 60.0;
    let second = chroma * (1.0 - (sector.rem_euclid(2.0) - 1.0).abs());
    let [r, g, b] = match sector as u8 {
        0 => [chroma, second, 0.0],
        1 => [second, chroma, 0.0],
        2 => [0.0, chroma, second],
        3 => [0.0, second, chroma],
        4 => [second, 0.0, chroma],
        _ => [chroma, 0.0, second],
    };
    let offset = lightness - chroma / 2.0;
    [r + offset, g + offset, b + offset]
}

/// A fully lit colour of the given hue, for picker tracks and gradients.
#[must_use]
pub fn hue_colour(hue: f32, saturation: f32) -> [f32; 3] {
    hsl_to_rgb(hue, saturation.clamp(0.0, 1.0), 0.5)
}

fn hue_distance(a: f32, b: f32) -> f32 {
    let distance = (a - b).rem_euclid(360.0);
    distance.min(360.0 - distance)
}

fn to_ycbcr([r, g, b]: [f32; 3]) -> [f32; 3] {
    let y = 0.114f32.mul_add(b, 0.299f32.mul_add(r, 0.587 * g));
    [y, (b - y) * 0.564, (r - y) * 0.713]
}

fn from_ycbcr([y, cb, cr]: [f32; 3]) -> [f32; 3] {
    [
        1.403f32.mul_add(cr, y),
        0.714f32.mul_add(-cr, 0.344f32.mul_add(-cb, y)),
        1.773f32.mul_add(cb, y),
    ]
}

/// The colour statistics of an image, sampled sparsely: a few hundred
/// thousand pixels describe a photo's palette as well as all of them do.
#[must_use]
pub fn colour_stats(pixels: &[u8], channels: usize) -> ColourStats {
    let step = (pixels.len() / channels / 250_000).max(1);
    let mut sum = [0.0f64; 3];
    let mut squares = [0.0f64; 3];
    let mut count = 0.0f64;
    for pixel in pixels.chunks_exact(channels).step_by(step) {
        let ycc = to_ycbcr([
            f32::from(pixel[0]) / 255.0,
            f32::from(pixel[1]) / 255.0,
            f32::from(pixel[2]) / 255.0,
        ]);
        for channel in 0..3 {
            let value = f64::from(ycc[channel]);
            sum[channel] += value;
            squares[channel] = value.mul_add(value, squares[channel]);
        }
        count += 1.0;
    }
    if count == 0.0 {
        return ColourStats::default();
    }
    let mean = sum.map(|total| total / count);
    let deviation: [f64; 3] = std::array::from_fn(|channel| {
        mean[channel]
            .mul_add(-mean[channel], squares[channel] / count)
            .max(0.0)
            .sqrt()
    });
    ColourStats {
        mean: mean.map(|value| value as f32),
        deviation: deviation.map(|value| value as f32),
    }
}

/// A 256-bin luminance histogram, for the Levels and Curves editors.
#[must_use]
pub fn luminance_histogram(pixels: &[u8], channels: usize) -> [u32; 256] {
    let mut bins = [0u32; 256];
    for pixel in pixels.chunks_exact(channels) {
        let value = luma([
            f32::from(pixel[0]),
            f32::from(pixel[1]),
            f32::from(pixel[2]),
        ]);
        bins[(value.round() as usize).min(255)] += 1;
    }
    bins
}

/// Monotone cubic interpolation (Fritsch–Carlson) through `points`, so a
/// curve never overshoots between its handles.
fn monotone_curve(xs: &[f32; CURVE_POINTS], ys: &[f32; CURVE_POINTS], x: f32) -> f32 {
    let n = CURVE_POINTS;
    let secants: Vec<f32> = (0..n - 1)
        .map(|i| (ys[i + 1] - ys[i]) / (xs[i + 1] - xs[i]))
        .collect();
    let mut tangents = vec![0.0f32; n];
    tangents[0] = secants[0];
    tangents[n - 1] = secants[n - 2];
    for i in 1..n - 1 {
        tangents[i] = if secants[i - 1] * secants[i] <= 0.0 {
            0.0
        } else {
            f32::midpoint(secants[i - 1], secants[i])
        };
    }
    for i in 0..n - 1 {
        if secants[i].abs() < 1e-6 {
            tangents[i] = 0.0;
            tangents[i + 1] = 0.0;
            continue;
        }
        let a = tangents[i] / secants[i];
        let b = tangents[i + 1] / secants[i];
        let length = a.hypot(b);
        if length > 3.0 {
            let scale = 3.0 / length;
            tangents[i] = scale * a * secants[i];
            tangents[i + 1] = scale * b * secants[i];
        }
    }
    let segment = (0..n - 1).find(|&i| x <= xs[i + 1]).unwrap_or(n - 2);
    let span = xs[segment + 1] - xs[segment];
    let t = ((x - xs[segment]) / span).clamp(0.0, 1.0);
    let t2 = t * t;
    let t3 = t2 * t;
    let h00 = 2.0f32.mul_add(t3, -3.0 * t2) + 1.0;
    let h10 = 2.0f32.mul_add(-t2, t3) + t;
    let h01 = (-2.0f32).mul_add(t3, 3.0 * t2);
    let h11 = t3 - t2;
    (h11 * span).mul_add(
        tangents[segment + 1],
        h01.mul_add(
            ys[segment + 1],
            (h10 * span).mul_add(tangents[segment], h00 * ys[segment]),
        ),
    )
}

impl Curve {
    /// The curve's output, 0 to 255, for an input from 0 to 255.
    #[must_use]
    pub fn sample(&self, input: f32) -> f32 {
        monotone_curve(&Self::INPUTS, &self.points, input).clamp(0.0, 255.0)
    }
}

impl Levels {
    fn sample(&self, input: f32) -> f32 {
        let span = (self.input_white - self.input_black).max(1.0);
        let normal = ((input - self.input_black) / span).clamp(0.0, 1.0);
        let toned = normal.powf(1.0 / self.gamma.max(0.01));
        toned.mul_add(self.output_white - self.output_black, self.output_black)
    }
}

// ----- colour lookup tables ------------------------------------------------

/// A parsed `.cube` 3D LUT, red varying fastest.
#[derive(Debug)]
pub struct Lut {
    size: usize,
    min: [f32; 3],
    max: [f32; 3],
    table: Vec<[f32; 3]>,
}

impl Lut {
    /// Parse the Adobe/Resolve `.cube` text format.
    ///
    /// # Errors
    ///
    /// Fails when the file has no `LUT_3D_SIZE`, a malformed row, or the
    /// wrong number of entries.
    pub fn parse(text: &str) -> Result<Self> {
        let mut size = None;
        let mut min = [0.0; 3];
        let mut max = [1.0; 3];
        let mut table = Vec::new();
        let triple = |parts: &[&str]| -> Result<[f32; 3]> {
            let values: Vec<f32> = parts
                .iter()
                .map(|part| part.parse::<f32>())
                .collect::<Result<_, _>>()
                .context("a LUT value is not a number")?;
            match values.as_slice() {
                [r, g, b] => Ok([*r, *g, *b]),
                _ => bail!("a LUT row needs three values"),
            }
        };
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let parts: Vec<&str> = line.split_whitespace().collect();
            match parts.first().copied() {
                Some("TITLE" | "LUT_1D_SIZE" | "LUT_1D_INPUT_RANGE") => {}
                Some("LUT_3D_SIZE") => {
                    let value: usize = parts
                        .get(1)
                        .context("LUT_3D_SIZE needs a value")?
                        .parse()
                        .context("LUT_3D_SIZE is not a number")?;
                    if !(2..=256).contains(&value) {
                        bail!("LUT_3D_SIZE {value} is out of range");
                    }
                    size = Some(value);
                }
                Some("DOMAIN_MIN") => min = triple(&parts[1..])?,
                Some("DOMAIN_MAX") => max = triple(&parts[1..])?,
                Some("LUT_3D_INPUT_RANGE") => {
                    let low: f32 = parts.get(1).context("missing range")?.parse()?;
                    let high: f32 = parts.get(2).context("missing range")?.parse()?;
                    min = [low; 3];
                    max = [high; 3];
                }
                _ => table.push(triple(&parts)?),
            }
        }
        let size = size.context("not a 3D LUT: LUT_3D_SIZE is missing")?;
        if table.len() != size * size * size {
            bail!(
                "the LUT lists {} entries; LUT_3D_SIZE {size} needs {}",
                table.len(),
                size * size * size
            );
        }
        Ok(Self {
            size,
            min,
            max,
            table,
        })
    }

    fn at(&self, r: usize, g: usize, b: usize) -> [f32; 3] {
        self.table[(b * self.size + g) * self.size + r]
    }

    /// Trilinear interpolation of the table at an sRGB colour.
    fn sample(&self, rgb: [f32; 3]) -> [f32; 3] {
        let last = (self.size - 1) as f32;
        let position: [f32; 3] = std::array::from_fn(|channel| {
            let span = (self.max[channel] - self.min[channel]).max(1e-6);
            ((rgb[channel] - self.min[channel]) / span).clamp(0.0, 1.0) * last
        });
        let low = position.map(|value| (value.floor() as usize).min(self.size - 2));
        let weight: [f32; 3] =
            std::array::from_fn(|channel| position[channel] - low[channel] as f32);
        let mut out = [0.0f32; 3];
        for corner in 0..8 {
            let pick = |bit: usize, channel: usize| low[channel] + ((corner >> bit) & 1);
            let factor = (0..3).fold(1.0, |factor: f32, channel| {
                if (corner >> channel) & 1 == 1 {
                    factor * weight[channel]
                } else {
                    factor * (1.0 - weight[channel])
                }
            });
            let value = self.at(pick(0, 0), pick(1, 1), pick(2, 2));
            for channel in 0..3 {
                out[channel] = value[channel].mul_add(factor, out[channel]);
            }
        }
        out
    }
}

fn lut_cache() -> &'static Mutex<HashMap<LutId, Arc<Lut>>> {
    static CACHE: OnceLock<Mutex<HashMap<LutId, Arc<Lut>>>> = OnceLock::new();
    CACHE.get_or_init(Mutex::default)
}

/// A stable id for a LUT file: FNV-1a over its path, so the same file keeps
/// the same id in every collection and across launches.
#[must_use]
pub fn lut_id(path: &Path) -> LutId {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in path.to_string_lossy().bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    LutId(hash)
}

fn load_lut(id: LutId, path: &Path) -> Result<()> {
    let text = std::fs::read_to_string(path).context("could not read the LUT file")?;
    let lut = Lut::parse(&text)?;
    lut_cache()
        .lock()
        .map_err(|_| anyhow::anyhow!("the LUT cache is poisoned"))?
        .insert(id, Arc::new(lut));
    Ok(())
}

/// Parse a `.cube` file into the shared LUT cache. Safe to call off the main
/// thread; record the result with [`AdjustmentMap::remember_lut`].
///
/// # Errors
///
/// Fails when the file cannot be read or is not a 3D LUT.
pub fn load_lut_file(path: &Path) -> Result<LutId> {
    let id = lut_id(path);
    load_lut(id, path)?;
    Ok(id)
}

fn cached_lut(id: LutId) -> Option<Arc<Lut>> {
    lut_cache().lock().ok()?.get(&id).cloned()
}

// ----- pipeline ------------------------------------------------------------

struct Matched {
    source: ColourStats,
    target: ColourStats,
    luminance: f32,
    intensity: f32,
    fade: f32,
}

/// Precomputed per-photo constants, so the per-pixel loop is table lookups
/// and a few multiplies. Each stage is `None` or skipped while neutral.
struct Pipeline {
    matched: Option<Matched>,
    /// Linear-light gains from exposure and colour balance.
    gains: [f32; 3],
    /// Linear to sRGB, sampled over 0..=1.
    encode: Vec<f32>,
    /// Brightness, contrast, levels and curve, sampled over 0..=1 in sRGB.
    tone: Vec<f32>,
    shadows: f32,
    highlights: f32,
    mixer: Option<[[f32; 3]; 3]>,
    filter: Option<([f32; 3], f32)>,
    selective: Option<[[f32; 3]; 6]>,
    replace: Option<ReplaceColour>,
    saturation: f32,
    vibrance: f32,
    hue: Option<[[f32; 3]; 3]>,
    monochrome: bool,
    gradient: Option<([f32; 3], [f32; 3], f32)>,
    lookup: Option<(Arc<Lut>, f32)>,
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
        // a symmetric power curve around mid-grey; levels and the curve follow.
        let gamma = (-adjustments.brightness / 100.0).exp2();
        let power = (adjustments.contrast / 100.0 * 1.5).exp2();
        let levels = adjustments.levels;
        let curve = adjustments.curve;
        let curve_active = curve != Curve::default();
        let tone = (0..=TONE_STEPS)
            .map(|step| {
                let value = (step as f32 / TONE_STEPS as f32).powf(gamma);
                let contrasted = if value < 0.5 {
                    0.5 * (2.0 * value).powf(power)
                } else {
                    0.5f32.mul_add(-2.0f32.mul_add(-value, 2.0).powf(power), 1.0)
                };
                let levelled = levels.sample(contrasted * 255.0);
                let curved = if curve_active {
                    curve.sample(levelled)
                } else {
                    levelled
                };
                curved / 255.0
            })
            .collect();

        let mixer = (adjustments.mixer != ChannelMixer::default()).then(|| {
            adjustments
                .mixer
                .rows
                .map(|row| row.map(|value| value / 100.0))
        });
        let selective = (adjustments.selective != SelectiveColour::default())
            .then_some(adjustments.selective.ranges);
        let gradient = adjustments.gradient_map.map(|map| {
            (
                hue_colour(map.shadow_hue, map.shadow_saturation / 100.0),
                hue_colour(map.highlight_hue, map.highlight_saturation / 100.0),
                map.amount / 100.0,
            )
        });
        let lookup = adjustments
            .lookup
            .and_then(|lookup| cached_lut(lookup.lut).map(|lut| (lut, lookup.amount / 100.0)));
        let hue = (adjustments.hue != 0.0).then(|| hue_matrix(adjustments.hue.to_radians()));
        Self {
            matched: adjustments.match_colour.map(|matched| Matched {
                source: ColourStats::default(),
                target: matched.target,
                luminance: matched.luminance / 100.0,
                intensity: matched.intensity / 100.0,
                fade: matched.fade / 100.0,
            }),
            gains,
            encode,
            tone,
            shadows: adjustments.shadows / 100.0,
            highlights: adjustments.highlights / 100.0,
            mixer,
            filter: adjustments
                .photo_filter
                .map(|filter| (hue_colour(filter.hue, 1.0), filter.density / 100.0)),
            selective,
            replace: adjustments.replace,
            saturation: adjustments.saturation / 100.0,
            vibrance: adjustments.vibrance / 100.0,
            hue,
            monochrome: adjustments.monochrome,
            gradient,
            lookup,
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
        let mut rgb = [r, g, b].map(|channel| f32::from(channel) / 255.0);

        if let Some(matched) = &self.matched {
            rgb = matched.transfer(rgb);
        }

        let decode = decode_table();
        let linear = rgb.map(|channel| (channel.clamp(0.0, 1.0) * 255.0).round() as usize);
        rgb = std::array::from_fn(|channel| {
            Self::sample(&self.encode, decode[linear[channel]] * self.gains[channel])
        });

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

        if let Some(rows) = &self.mixer {
            rgb = rows.map(|row| row[2].mul_add(rgb[2], row[0].mul_add(rgb[0], row[1] * rgb[1])));
        }

        if let Some((colour, density)) = self.filter {
            // A lens filter tints toward its colour but keeps luminosity.
            let y = luma(rgb);
            let tinted = lerp(rgb, colour, density * 0.6);
            let scale = y / luma(tinted).max(1e-4);
            rgb = tinted.map(|channel| channel * scale);
        }

        if let Some(ranges) = &self.selective {
            rgb = selective_colour(rgb, ranges);
        }
        if let Some(replace) = &self.replace {
            rgb = replace_colour(rgb, replace);
        }

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

        if let Some((shadow, highlight, amount)) = self.gradient {
            rgb = lerp(rgb, gradient_tint(shadow, highlight, luma(rgb)), amount);
        }

        if let Some((lut, amount)) = &self.lookup {
            let clamped = rgb.map(|channel| channel.clamp(0.0, 1.0));
            rgb = lerp(clamped, lut.sample(clamped), *amount);
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

impl Matched {
    /// Reinhard-style transfer in `YCbCr`: shift and scale each channel from
    /// this photo's statistics to the source's.
    fn transfer(&self, rgb: [f32; 3]) -> [f32; 3] {
        let ycc = to_ycbcr(rgb);
        let moved: [f32; 3] = std::array::from_fn(|channel| {
            let spread = if self.source.deviation[channel] > 1e-4 {
                (self.target.deviation[channel] / self.source.deviation[channel]).clamp(0.25, 4.0)
            } else {
                1.0
            };
            (ycc[channel] - self.source.mean[channel]).mul_add(spread, self.target.mean[channel])
        });
        let matched = from_ycbcr([
            moved[0] * self.luminance,
            moved[1] * self.intensity,
            moved[2] * self.intensity,
        ]);
        lerp(matched, rgb, self.fade)
    }
}

/// Each hue family's weight falls off linearly to zero 60° away, so the six
/// weights always sum to one and adjacent families blend smoothly.
fn selective_colour(rgb: [f32; 3], ranges: &[[f32; 3]; 6]) -> [f32; 3] {
    let (hue, saturation, lightness) = rgb_to_hsl(rgb);
    if saturation < 0.02 {
        return rgb;
    }
    let mut shift = [0.0f32; 3];
    for range in HueRange::ALL {
        let weight = (1.0 - hue_distance(hue, range.centre()) / 60.0).max(0.0);
        for knob in 0..3 {
            shift[knob] = (ranges[range.index()][knob] / 100.0).mul_add(weight, shift[knob]);
        }
    }
    // A family's full hue shift is 30°, a sixth of replace colour's.
    shift_hsl(
        hue,
        saturation,
        lightness,
        [shift[0] / 6.0, shift[1], shift[2]],
        saturation.min(1.0),
    )
}

fn replace_colour(rgb: [f32; 3], replace: &ReplaceColour) -> [f32; 3] {
    let (hue, saturation, lightness) = rgb_to_hsl(rgb);
    if saturation < 0.05 {
        return rgb;
    }
    let weight = (1.0 - hue_distance(hue, replace.colour) / replace.range.max(1.0)).max(0.0);
    if weight <= 0.0 {
        return rgb;
    }
    shift_hsl(
        hue,
        saturation,
        lightness,
        [
            replace.hue / 180.0,
            replace.saturation / 100.0,
            replace.lightness / 100.0,
        ],
        weight * saturation.min(1.0).sqrt(),
    )
}

/// Apply `[hue, saturation, lightness]` shifts, each from -1 to 1, scaled by
/// `strength`. A hue shift of 1 turns 180°.
fn shift_hsl(
    hue: f32,
    saturation: f32,
    lightness: f32,
    [dh, ds, dl]: [f32; 3],
    strength: f32,
) -> [f32; 3] {
    let hue = (dh * 180.0).mul_add(strength, hue);
    let saturation = (saturation * ds.mul_add(strength, 1.0)).clamp(0.0, 1.0);
    let lightness = (dl * 0.5).mul_add(strength, lightness).clamp(0.0, 1.0);
    hsl_to_rgb(hue, saturation, lightness)
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

// ----- per-collection storage ----------------------------------------------

/// The adjusted photos of a collection and the LUT files they use.
///
/// Most photos carry no adjustments, so this is sparse. It sits behind
/// `Arc`s so whole-collection undo snapshots share it, and photos given the
/// same adjustments in one copy share one value.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct AdjustmentMap {
    photos: Arc<BTreeMap<PhotoId, Arc<Adjustments>>>,
    luts: Arc<BTreeMap<LutId, PathBuf>>,
}

impl AdjustmentMap {
    #[must_use]
    pub fn get(&self, id: PhotoId) -> Option<&Adjustments> {
        self.photos.get(&id).map(AsRef::as_ref)
    }

    /// The photo's adjustments, or the identity when it has none.
    #[must_use]
    pub fn for_photo(&self, id: PhotoId) -> Adjustments {
        self.get(id).copied().unwrap_or_default()
    }

    /// The stored value itself, so undo and bulk copies can share it.
    #[must_use]
    pub fn shared(&self, id: PhotoId) -> Option<Arc<Adjustments>> {
        self.photos.get(&id).cloned()
    }

    /// Store `adjustments` for `id`; neutral adjustments remove the entry so
    /// the collection file only lists photos that are actually adjusted.
    pub fn set(&mut self, id: PhotoId, adjustments: Adjustments) {
        self.set_shared(id, Arc::new(adjustments.normalized()));
    }

    pub fn set_shared(&mut self, id: PhotoId, adjustments: Arc<Adjustments>) {
        if adjustments.is_neutral() {
            if self.photos.contains_key(&id) {
                Arc::make_mut(&mut self.photos).remove(&id);
            }
        } else if self.photos.get(&id) != Some(&adjustments) {
            Arc::make_mut(&mut self.photos).insert(id, adjustments);
        }
    }

    pub fn retain(&mut self, mut keep: impl FnMut(PhotoId) -> bool) {
        if self.photos.keys().any(|id| !keep(*id)) {
            Arc::make_mut(&mut self.photos).retain(|id, _| keep(*id));
        }
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.photos.is_empty()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.photos.len()
    }

    /// Remember a LUT file loaded with [`load_lut_file`], so the collection
    /// can load it again on open.
    pub fn remember_lut(&mut self, id: LutId, path: &Path) {
        if self.luts.get(&id).map(PathBuf::as_path) != Some(path) {
            Arc::make_mut(&mut self.luts).insert(id, path.to_path_buf());
        }
    }

    #[must_use]
    pub fn lut_path(&self, id: LutId) -> Option<&Path> {
        self.luts.get(&id).map(PathBuf::as_path)
    }

    #[must_use]
    pub fn lut_loaded(id: LutId) -> bool {
        cached_lut(id).is_some()
    }

    /// Load every LUT this collection references, returning how many could
    /// not be read. A missing LUT leaves its photos unlooked-up rather than
    /// failing the whole collection.
    #[must_use]
    pub fn load_luts(&self) -> usize {
        self.luts
            .iter()
            .filter(|(id, path)| load_lut(**id, path).is_err())
            .count()
    }
}

#[derive(Serialize, Deserialize, Default)]
#[serde(default)]
struct AdjustmentWire {
    photos: BTreeMap<PhotoId, Adjustments>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    luts: BTreeMap<LutId, PathBuf>,
}

impl Serialize for AdjustmentMap {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        AdjustmentWire {
            photos: self
                .photos
                .iter()
                .map(|(id, adjustments)| (*id, **adjustments))
                .collect(),
            luts: self.luts.as_ref().clone(),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for AdjustmentMap {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = AdjustmentWire::deserialize(deserializer)?;
        Ok(Self {
            photos: Arc::new(
                wire.photos
                    .into_iter()
                    .map(|(id, adjustments)| (id, adjustments.normalized()))
                    .filter(|(_, adjustments)| !adjustments.is_neutral())
                    .map(|(id, adjustments)| (id, Arc::new(adjustments)))
                    .collect(),
            ),
            luts: Arc::new(wire.luts),
        })
    }
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;

    use super::*;

    fn rgb(adjustments: &Adjustments, pixel: [u8; 3]) -> [u8; 3] {
        let mut pixels = pixel.to_vec();
        adjustments.apply_rgb(&mut pixels);
        [pixels[0], pixels[1], pixels[2]]
    }

    fn close(a: [u8; 3], b: [u8; 3], tolerance: u8) -> bool {
        (0..3).all(|channel| a[channel].abs_diff(b[channel]) <= tolerance)
    }

    #[test]
    fn neutral_adjustments_leave_pixels_untouched() {
        let adjustments = Adjustments::default();
        assert!(adjustments.is_neutral());
        for pixel in [[0, 0, 0], [12, 128, 250], [255, 255, 255]] {
            assert_eq!(rgb(&adjustments, pixel), pixel);
        }
    }

    #[test]
    fn a_non_neutral_pipeline_round_trips_mid_tones() {
        let adjustments = Adjustments {
            saturation: 1.0,
            ..Adjustments::default()
        };
        assert!(close(rgb(&adjustments, [90, 140, 200]), [90, 140, 200], 2));
        assert!(close(rgb(&adjustments, [6, 9, 12]), [6, 9, 12], 1));
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
    fn levels_remap_the_input_range() {
        let mut adjustments = Adjustments::default();
        adjustments.set_slider(Slider::Levels(LevelsKnob::InputBlack), 50.0);
        adjustments.set_slider(Slider::Levels(LevelsKnob::InputWhite), 200.0);
        assert_eq!(rgb(&adjustments, [50, 50, 50]), [0, 0, 0]);
        assert_eq!(rgb(&adjustments, [200, 200, 200]), [255, 255, 255]);
        // Black can never cross white.
        adjustments.set_slider(Slider::Levels(LevelsKnob::InputBlack), 250.0);
        assert!(adjustments.levels.input_black < adjustments.levels.input_white);
    }

    #[test]
    fn the_default_curve_is_the_identity_and_curves_stay_monotone() {
        let curve = Curve::default();
        for input in [0.0, 17.0, 128.0, 240.0, 255.0] {
            assert!((curve.sample(input) - input).abs() < 1.0);
        }
        let steep = Curve {
            points: [0.0, 10.0, 200.0, 210.0, 255.0],
        };
        let mut last = 0.0;
        for input in 0..=255 {
            let output = steep.sample(input as f32);
            assert!(output + 1e-3 >= last, "curve dipped at {input}");
            last = output;
        }
    }

    #[test]
    fn the_channel_mixer_can_swap_channels() {
        let mut adjustments = Adjustments::default();
        adjustments.mixer.rows = [[0.0, 0.0, 100.0], [0.0, 100.0, 0.0], [100.0, 0.0, 0.0]];
        assert!(close(rgb(&adjustments, [200, 100, 20]), [20, 100, 200], 2));
    }

    #[test]
    fn colour_shifts_leave_greys_alone() {
        let mut adjustments = Adjustments {
            replace: Some(ReplaceColour {
                colour: 0.0,
                range: 40.0,
                hue: 120.0,
                saturation: 0.0,
                lightness: 0.0,
            }),
            ..Adjustments::default()
        };
        adjustments.selective.ranges[HueRange::Blues.index()] = [50.0, 50.0, 0.0];
        assert!(close(
            rgb(&adjustments, [128, 128, 128]),
            [128, 128, 128],
            1
        ));
        let red = rgb(&adjustments, [220, 30, 30]);
        assert!(
            red[0] < red[1] || red[0] < red[2],
            "red was not replaced: {red:?}"
        );
    }

    #[test]
    fn a_grey_gradient_map_is_black_and_white() {
        let adjustments = Adjustments {
            gradient_map: Some(GradientMap {
                shadow_saturation: 0.0,
                highlight_saturation: 0.0,
                ..GradientMap::default()
            }),
            ..Adjustments::default()
        };
        let [r, g, b] = rgb(&adjustments, [200, 40, 40]);
        assert!(r.abs_diff(g) <= 1 && g.abs_diff(b) <= 1);
    }

    #[test]
    fn match_colour_moves_toward_the_source_palette() {
        let warm = ColourStats {
            mean: [0.5, -0.1, 0.15],
            deviation: [0.2, 0.05, 0.05],
        };
        let adjustments = Adjustments {
            match_colour: Some(MatchColour::new(PhotoId(9), warm)),
            ..Adjustments::default()
        };
        let mut pixels = vec![80, 120, 200, 90, 130, 210, 70, 110, 190];
        adjustments.apply_rgb(&mut pixels);
        assert!(
            pixels[0] > pixels[2],
            "the blue image did not warm up: {pixels:?}"
        );
    }

    #[test]
    fn cube_luts_parse_and_an_identity_lut_is_transparent() {
        let mut text = String::from("TITLE \"identity\"\nLUT_3D_SIZE 2\n");
        for b in 0..2 {
            for g in 0..2 {
                for r in 0..2 {
                    let _ = writeln!(text, "{r} {g} {b}");
                }
            }
        }
        let lut = Lut::parse(&text).unwrap();
        let sample = lut.sample([0.25, 0.5, 0.75]);
        assert!((sample[0] - 0.25).abs() < 1e-4 && (sample[2] - 0.75).abs() < 1e-4);
        assert!(Lut::parse("LUT_3D_SIZE 2\n0 0 0\n").is_err());
    }

    #[test]
    fn hsl_round_trips() {
        for colour in [[0.9, 0.2, 0.1], [0.1, 0.6, 0.3], [0.2, 0.3, 0.9]] {
            let (h, s, l) = rgb_to_hsl(colour);
            let back = hsl_to_rgb(h, s, l);
            for channel in 0..3 {
                assert!((back[channel] - colour[channel]).abs() < 1e-4);
            }
        }
    }

    #[test]
    fn log_sliders_centre_their_neutral() {
        let range = Slider::Levels(LevelsKnob::Gamma).range();
        let middle = range.fraction(1.0);
        assert!((middle - 0.5).abs() < 0.01, "{middle}");
        assert!((range.value_at(middle) - 1.0).abs() < 0.02);
    }

    #[test]
    fn normalising_clamps_hand_edited_values_without_enabling_groups() {
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
        assert!(adjustments.photo_filter.is_none() && adjustments.replace.is_none());
        assert!(Adjustments::default().normalized().is_neutral());
    }

    #[test]
    fn the_map_drops_neutral_entries_and_round_trips() {
        let mut map = AdjustmentMap::default();
        map.set(
            PhotoId(3),
            Adjustments {
                exposure: 0.5,
                monochrome: true,
                photo_filter: Some(PhotoFilter::default()),
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
        assert!(Arc::ptr_eq(&map.photos, &snapshot.photos));
        map.set(
            PhotoId(2),
            Adjustments {
                invert: true,
                ..Adjustments::default()
            },
        );
        assert!(!Arc::ptr_eq(&map.photos, &snapshot.photos));
        assert_eq!(snapshot.len(), 1);
    }
}
