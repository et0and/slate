//! Shared visual language: monochrome, high contrast, light and dark.

use std::sync::atomic::{AtomicBool, Ordering};

use gpui::{Rgba, rgb};

#[cfg(target_os = "macos")]
pub const FONT_UI: &str = "Menlo";
#[cfg(target_os = "windows")]
pub const FONT_UI: &str = "Consolas";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const FONT_UI: &str = "DejaVu Sans Mono";

#[cfg(target_os = "macos")]
pub const FONT_MONO: &str = "Menlo";
#[cfg(target_os = "windows")]
pub const FONT_MONO: &str = "Consolas";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
pub const FONT_MONO: &str = "DejaVu Sans Mono";

static DARK: AtomicBool = AtomicBool::new(false);

pub fn set_dark(dark: bool) {
    DARK.store(dark, Ordering::Relaxed);
}

pub fn is_dark() -> bool {
    DARK.load(Ordering::Relaxed)
}

fn tone(light: u32, dark: u32) -> Rgba {
    if is_dark() { rgb(dark) } else { rgb(light) }
}

pub fn ink() -> Rgba {
    tone(0x111111, 0xf3f3f1)
}

pub fn paper() -> Rgba {
    tone(0xf6f6f4, 0x0b0b0c)
}

pub fn panel() -> Rgba {
    tone(0xfbfbfa, 0x111113)
}

pub fn surface() -> Rgba {
    tone(0xffffff, 0x171719)
}

/// A sheet is paper: always white, in both interface appearances.
pub fn sheet() -> Rgba {
    rgb(0xffffff)
}

pub fn sheet_ink() -> Rgba {
    rgb(0x0a0a0a)
}

pub fn sheet_muted() -> Rgba {
    rgb(0x555555)
}

pub fn sheet_photo() -> Rgba {
    rgb(0xededed)
}

pub fn sheet_rule() -> Rgba {
    rgb(0xd0d0d0)
}

pub fn muted() -> Rgba {
    tone(0x5f5f5c, 0xb3b3af)
}

pub fn faint() -> Rgba {
    tone(0x8b8b87, 0x858581)
}

pub fn rule() -> Rgba {
    tone(0xe3e3df, 0x2a2a2c)
}

/// Quiet fill for hover, selection and disabled emphasis.
pub fn accent_soft() -> Rgba {
    tone(0xeeeeeb, 0x222224)
}

pub fn photo_bg() -> Rgba {
    tone(0xefefec, 0x1d1d1f)
}

pub const FIELD: f32 = 30.0;
