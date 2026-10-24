//! Shared visual language: monochrome by default, with optional palettes.

use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Rgba, rgb};

/// Geist is loaded from `assets/fonts` at startup. Geist Mono carries data,
/// shortcuts and technical metadata; Geist carries names, labels and actions.
pub const FONT_UI: &str = "Geist";
pub const FONT_MONO: &str = "Geist Mono";

pub const TEXT_TITLE: f32 = 12.5;
pub const TEXT_BODY: f32 = 11.0;
pub const TEXT_LABEL: f32 = 10.0;
pub const TEXT_META: f32 = 9.0;
pub const TEXT_MICRO: f32 = 8.5;

pub const CONTROL_H: f32 = 30.0;
pub const ROW_H: f32 = 32.0;
pub const TOPBAR_H: f32 = 44.0;
pub const STATUS_H: f32 = 30.0;
pub const FIELD: f32 = 32.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    OneDark,
    Monokai,
    Catppuccin,
}

impl Theme {
    pub fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::OneDark => "One Dark",
            Self::Monokai => "Monokai",
            Self::Catppuccin => "Catppuccin",
        }
    }

    fn index(self) -> u8 {
        match self {
            Self::Light => 0,
            Self::Dark => 1,
            Self::OneDark => 2,
            Self::Monokai => 3,
            Self::Catppuccin => 4,
        }
    }

    fn from_index(index: u8) -> Self {
        match index {
            1 => Self::Dark,
            2 => Self::OneDark,
            3 => Self::Monokai,
            4 => Self::Catppuccin,
            _ => Self::Light,
        }
    }

    fn palette(self) -> Palette {
        match self {
            Self::Light => Palette {
                ink: 0x111111,
                paper: 0xf6f6f4,
                panel: 0xfbfbfa,
                surface: 0xffffff,
                muted: 0x5f5f5c,
                faint: 0x8b8b87,
                rule: 0xe3e3df,
                soft: 0xeeeeeb,
                photo: 0xefefec,
                accent: 0x111111,
                accent_text: 0xf6f6f4,
            },
            Self::Dark => Palette {
                ink: 0xf3f3f1,
                paper: 0x0b0b0c,
                panel: 0x111113,
                surface: 0x171719,
                muted: 0xb3b3af,
                faint: 0x858581,
                rule: 0x2a2a2c,
                soft: 0x222224,
                photo: 0x1d1d1f,
                accent: 0xf3f3f1,
                accent_text: 0x0b0b0c,
            },
            Self::OneDark => Palette {
                ink: 0xd7dae0,
                paper: 0x21252b,
                panel: 0x282c34,
                surface: 0x2f343d,
                muted: 0xa7afbd,
                faint: 0x7f8795,
                rule: 0x3b4048,
                soft: 0x323842,
                photo: 0x2b3038,
                accent: 0x61afef,
                accent_text: 0x11151b,
            },
            Self::Monokai => Palette {
                ink: 0xf8f8f2,
                paper: 0x1f1f1c,
                panel: 0x272822,
                surface: 0x2f3028,
                muted: 0xc5c5b9,
                faint: 0x8a8b80,
                rule: 0x46473d,
                soft: 0x35362e,
                photo: 0x2d2e26,
                accent: 0xf92672,
                accent_text: 0x1f1f1c,
            },
            Self::Catppuccin => Palette {
                ink: 0xcdd6f4,
                paper: 0x11111b,
                panel: 0x181825,
                surface: 0x1e1e2e,
                muted: 0xa6adc8,
                faint: 0x7f849c,
                rule: 0x313244,
                soft: 0x292938,
                photo: 0x24243a,
                accent: 0xcba6f7,
                accent_text: 0x11111b,
            },
        }
    }
}

struct Palette {
    ink: u32,
    paper: u32,
    panel: u32,
    surface: u32,
    muted: u32,
    faint: u32,
    rule: u32,
    soft: u32,
    photo: u32,
    accent: u32,
    accent_text: u32,
}

static THEME: AtomicU8 = AtomicU8::new(0);

pub fn set_theme(theme: Theme) {
    THEME.store(theme.index(), Ordering::Relaxed);
}

pub fn current_theme() -> Theme {
    Theme::from_index(THEME.load(Ordering::Relaxed))
}

fn palette() -> Palette {
    current_theme().palette()
}

pub fn ink() -> Rgba {
    rgb(palette().ink)
}

pub fn paper() -> Rgba {
    rgb(palette().paper)
}

pub fn panel() -> Rgba {
    rgb(palette().panel)
}

pub fn surface() -> Rgba {
    rgb(palette().surface)
}

/// A sheet is paper: always white, in every interface palette.
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
    rgb(palette().muted)
}

pub fn faint() -> Rgba {
    rgb(palette().faint)
}

pub fn rule() -> Rgba {
    rgb(palette().rule)
}

/// Quiet fill for hover, selection and disabled emphasis.
pub fn accent_soft() -> Rgba {
    rgb(palette().soft)
}

pub fn accent() -> Rgba {
    rgb(palette().accent)
}

pub fn accent_text() -> Rgba {
    rgb(palette().accent_text)
}

pub fn photo_bg() -> Rgba {
    rgb(palette().photo)
}
