//! Shared visual language: flat, dense, black and paper with one accent.

use gpui::{Rgba, rgb, rgba};

pub const FONT_UI: &str = "Helvetica Neue";
pub const FONT_MONO: &str = "Menlo";

pub fn ink() -> Rgba {
    rgb(0x111110)
}

pub fn paper() -> Rgba {
    rgb(0xfbfaf6)
}

pub fn panel() -> Rgba {
    rgb(0xf2f0e9)
}

pub fn white() -> Rgba {
    rgb(0xffffff)
}

pub fn muted() -> Rgba {
    rgb(0x6f6c64)
}

pub fn faint() -> Rgba {
    rgb(0xa8a49a)
}

pub fn rule() -> Rgba {
    rgb(0xd8d4ca)
}

pub fn accent() -> Rgba {
    rgb(0xe43d0f)
}

pub fn accent_soft() -> Rgba {
    rgba(0xe43d0f14)
}

pub fn danger() -> Rgba {
    rgb(0xb42304)
}

pub fn photo_bg() -> Rgba {
    rgb(0xe8e5dd)
}

pub const MICRO: f32 = 9.0;
pub const FIELD: f32 = 26.0;
