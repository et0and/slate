//! Bundled fonts and monochrome SVG icons.

use std::borrow::Cow;

use anyhow::Result;
use gpui::{App, AssetSource, SharedString};

macro_rules! icons {
    ($($name:ident => $path:literal),* $(,)?) => {
        $(pub const $name: &str = $path;)*

        pub fn load_icon(path: &str) -> Option<Cow<'static, [u8]>> {
            match path {
                $($path => Some(Cow::Borrowed(
                    include_bytes!(concat!("../../../assets/", $path)).as_slice(),
                )),)*
                _ => None,
            }
        }
    };
}

icons! {
    CHECK => "icons/check.svg",
    CHEVRON_LEFT => "icons/chevron-left.svg",
    CHEVRON_RIGHT => "icons/chevron-right.svg",
    PLUS => "icons/plus.svg",
    MINUS => "icons/minus.svg",
    X => "icons/x.svg",
    FOLDER => "icons/folder.svg",
    FOLDER_PLUS => "icons/folder-plus.svg",
    FILE_PLUS => "icons/file-plus.svg",
    SAVE => "icons/save.svg",
    DOWNLOAD => "icons/download.svg",
    UPLOAD => "icons/upload.svg",
    TRASH => "icons/trash.svg",
    EXTERNAL_LINK => "icons/external-link.svg",
    IMAGE => "icons/image.svg",
    IMAGE_OFF => "icons/image-off.svg",
    SEARCH_X => "icons/search-x.svg",
    ALERT => "icons/alert.svg",
    ARROW_RIGHT => "icons/arrow-right.svg",
    ROTATE => "icons/rotate.svg",
    UNDO => "icons/undo.svg",
    REDO => "icons/redo.svg",
    CLOCK => "icons/clock.svg",
    INFO => "icons/info.svg",
    LAYOUT => "icons/layout.svg",
}

pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        Ok(load_icon(path))
    }

    fn list(&self, _path: &str) -> Result<Vec<SharedString>> {
        Ok(Vec::new())
    }
}

/// Load the bundled Geist faces before the first window opens.
pub fn install_fonts(cx: &App) {
    let fonts: Vec<Cow<'static, [u8]>> = vec![
        Cow::Borrowed(include_bytes!("../../../assets/fonts/Geist-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/Geist-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/Geist-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/Geist-Bold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/GeistMono-Regular.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/GeistMono-Medium.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/GeistMono-SemiBold.ttf").as_slice()),
        Cow::Borrowed(include_bytes!("../../../assets/fonts/GeistMono-Bold.ttf").as_slice()),
    ];
    cx.text_system()
        .add_fonts(fonts)
        .expect("bundled Geist fonts load");
}
