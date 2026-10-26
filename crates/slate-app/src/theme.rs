//! Shared visual language: monochrome by default, with optional palettes.

use std::sync::atomic::{AtomicU8, Ordering};

use gpui::{Rgba, rgb};

/// Names, labels and actions use the platform's own interface font: San
/// Francisco on macOS, the system UI font (normally Segoe UI) on Windows and
/// the desktop sans on Linux. Bundled Geist Mono carries data, shortcuts and
/// technical metadata, so figures line up the same on every platform.
pub const FONT_UI: &str = ".SystemUIFont";
pub const FONT_MONO: &str = "Geist Mono";

/// macOS text styles, rounded to the nearest practical size. Use these five
/// steps only; never offset a token by a fraction.
///
/// - Display: one per surface — dialog, window and empty-state headings.
/// - Title: toolbar headings inside a window.
/// - Body: names, menu rows, buttons, running copy and shortcut hints.
/// - Label: section headings, field labels and compact controls.
/// - Meta: counts, sizes, dates and footnotes.
///
/// Weight encodes the role, never the state: headings are semibold,
/// controls are medium, content is regular. Selection, focus and disabled
/// states change colour or fill, so text never reflows when it changes.
pub const TEXT_DISPLAY: f32 = 20.0;
pub const TEXT_TITLE: f32 = 15.0;
pub const TEXT_BODY: f32 = 13.0;
pub const TEXT_LABEL: f32 = 11.0;
pub const TEXT_META: f32 = 10.0;

pub const CONTROL_H: f32 = 32.0;
pub const ROW_H: f32 = 36.0;
pub const TOPBAR_H: f32 = 46.0;
pub const STATUS_H: f32 = 32.0;
pub const FIELD: f32 = 34.0;

/// Space at the top of the window for the native traffic lights when the
/// titlebar is transparent. Other platforms keep their in-window top bar.
#[cfg(target_os = "macos")]
pub const WINDOW_CHROME_TOP: f32 = 38.0;
#[cfg(not(target_os = "macos"))]
pub const WINDOW_CHROME_TOP: f32 = 0.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Theme {
    Light,
    Dark,
    OneDark,
    Nord,
    SolarizedLight,
    SolarizedDark,
    TokyoNight,
    Catppuccin,
    RosePine,
    Gruvbox,
    Monokai,
}

impl Theme {
    pub const ALL: [Self; 11] = [
        Self::Light,
        Self::Dark,
        Self::OneDark,
        Self::Nord,
        Self::SolarizedLight,
        Self::SolarizedDark,
        Self::TokyoNight,
        Self::Catppuccin,
        Self::RosePine,
        Self::Gruvbox,
        Self::Monokai,
    ];

    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::OneDark => "One Dark",
            Self::Nord => "Nord",
            Self::SolarizedLight => "Solarized Light",
            Self::SolarizedDark => "Solarized Dark",
            Self::TokyoNight => "Tokyo Night",
            Self::Catppuccin => "Catppuccin",
            Self::RosePine => "Rosé Pine",
            Self::Gruvbox => "Gruvbox",
            Self::Monokai => "Monokai",
        }
    }

    pub const fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::OneDark => "one-dark",
            Self::Nord => "nord",
            Self::SolarizedLight => "solarized-light",
            Self::SolarizedDark => "solarized-dark",
            Self::TokyoNight => "tokyo-night",
            Self::Catppuccin => "catppuccin",
            Self::RosePine => "rose-pine",
            Self::Gruvbox => "gruvbox",
            Self::Monokai => "monokai",
        }
    }

    pub fn from_key(key: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|theme| theme.key() == key)
    }

    #[cfg(any(target_os = "macos", test))]
    pub const fn is_dark(self) -> bool {
        !matches!(self, Self::Light | Self::SolarizedLight)
    }

    fn index(self) -> u8 {
        Self::ALL
            .iter()
            .position(|theme| *theme == self)
            .unwrap_or(0) as u8
    }

    fn from_index(index: u8) -> Self {
        Self::ALL
            .get(index as usize)
            .copied()
            .unwrap_or(Self::Light)
    }

    const fn palette(self) -> Palette {
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
            Self::Nord => Palette {
                ink: 0xeceff4,
                paper: 0x2e3440,
                panel: 0x292e39,
                surface: 0x3b4252,
                muted: 0xd8dee9,
                faint: 0x7b88a1,
                rule: 0x434c5e,
                soft: 0x353c4a,
                photo: 0x333a47,
                accent: 0x88c0d0,
                accent_text: 0x2e3440,
            },
            Self::SolarizedLight => Palette {
                ink: 0x073642,
                paper: 0xfdf6e3,
                panel: 0xf7f0dd,
                surface: 0xffffff,
                muted: 0x586e75,
                faint: 0x93a1a1,
                rule: 0xe6dfcc,
                soft: 0xeee8d5,
                photo: 0xe9e3d0,
                accent: 0x268bd2,
                accent_text: 0xfdf6e3,
            },
            Self::SolarizedDark => Palette {
                ink: 0xeee8d5,
                paper: 0x002b36,
                panel: 0x073642,
                surface: 0x0a3a46,
                muted: 0x93a1a1,
                faint: 0x657b83,
                rule: 0x174b57,
                soft: 0x12404b,
                photo: 0x0e3944,
                accent: 0x2aa198,
                accent_text: 0x002b36,
            },
            Self::TokyoNight => Palette {
                ink: 0xc0caf5,
                paper: 0x1a1b26,
                panel: 0x16161e,
                surface: 0x24283b,
                muted: 0x9aa5ce,
                faint: 0x565f89,
                rule: 0x2f334d,
                soft: 0x292e42,
                photo: 0x202333,
                accent: 0x7aa2f7,
                accent_text: 0x1a1b26,
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
            Self::RosePine => Palette {
                ink: 0xe0def4,
                paper: 0x191724,
                panel: 0x1f1d2e,
                surface: 0x26233a,
                muted: 0x908caa,
                faint: 0x6e6a86,
                rule: 0x403d52,
                soft: 0x2a273f,
                photo: 0x21202e,
                accent: 0xc4a7e7,
                accent_text: 0x191724,
            },
            Self::Gruvbox => Palette {
                ink: 0xebdbb2,
                paper: 0x1d2021,
                panel: 0x282828,
                surface: 0x32302f,
                muted: 0xbdae93,
                faint: 0x928374,
                rule: 0x504945,
                soft: 0x3c3836,
                photo: 0x2f2e2b,
                accent: 0xfabd2f,
                accent_text: 0x1d2021,
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

/// Fixed label colours. These are the one intentional colour exception to the
/// monochrome interface, because colour labels must stay recognisable across
/// themes.
pub fn label_color(label: slate_core::model::ColorLabel) -> Rgba {
    use slate_core::model::ColorLabel;
    match label {
        ColorLabel::None => accent_soft(),
        ColorLabel::Red => rgb(0xd34a45),
        ColorLabel::Yellow => rgb(0xd8a52e),
        ColorLabel::Green => rgb(0x4d9a5e),
        ColorLabel::Blue => rgb(0x4a78c8),
        ColorLabel::Purple => rgb(0x8f63c9),
    }
}

/// Match the native window chrome to the app palette. Without this, macOS
/// keeps the titlebar in the system appearance even when the interface is
/// explicitly light or dark.
#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub fn configure_window_chrome() {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let windows: *mut Object = msg_send![app, windows];
        let count: usize = msg_send![windows, count];
        for index in 0..count {
            let window: *mut Object = msg_send![windows, objectAtIndex: index];
            if window.is_null() {
                continue;
            }
            let () = msg_send![window, setTitlebarAppearsTransparent: true];
            let () = msg_send![window, setTitlebarSeparatorStyle: 0usize];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub const fn configure_window_chrome() {}

#[cfg(target_os = "macos")]
#[allow(unexpected_cfgs)]
pub fn apply_native_appearance(theme: Theme) {
    use objc::runtime::Object;
    use objc::{class, msg_send, sel, sel_impl};

    #[link(name = "AppKit", kind = "framework")]
    unsafe extern "C" {
        static NSAppearanceNameAqua: *mut Object;
        static NSAppearanceNameDarkAqua: *mut Object;
    }

    unsafe {
        let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
        let name = if theme.is_dark() {
            NSAppearanceNameDarkAqua
        } else {
            NSAppearanceNameAqua
        };
        let appearance: *mut Object = msg_send![class!(NSAppearance), appearanceNamed: name];
        if !app.is_null() && !appearance.is_null() {
            let () = msg_send![app, setAppearance: appearance];
        }
    }
}

#[cfg(not(target_os = "macos"))]
pub const fn apply_native_appearance(_theme: Theme) {}

#[cfg(test)]
mod tests {
    use super::Theme;

    #[test]
    fn every_theme_has_a_stable_index() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_index(theme.index()), theme);
        }
    }

    #[test]
    fn every_theme_round_trips_through_its_storage_key() {
        for theme in Theme::ALL {
            assert_eq!(Theme::from_key(theme.key()), Some(theme));
        }
    }

    #[test]
    fn light_theme_names_are_unique() {
        for (index, theme) in Theme::ALL.iter().enumerate() {
            assert!(
                !Theme::ALL
                    .iter()
                    .skip(index + 1)
                    .any(|other| other.label() == theme.label())
            );
        }
    }

    #[test]
    fn only_the_light_palettes_are_light() {
        for theme in Theme::ALL {
            assert_eq!(
                theme.is_dark(),
                !matches!(theme, Theme::Light | Theme::SolarizedLight)
            );
        }
    }
}
