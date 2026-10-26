#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

mod about_window;
mod adjust_ui;
mod app;
mod assets;
mod chrome;
mod drive;
mod drive_ui;
mod history;
mod inspector;
mod library;
mod memory;
mod menubar;
mod overlay;
mod panels;
mod photo_preview;
mod preview_window;
mod rename_window;
mod selection;
mod sidebar;
mod telemetry;
mod text_field;
mod theme;
mod view;
mod widgets;

use std::path::PathBuf;

use anyhow::Result;
use gpui::{
    App, Application, Bounds, KeyBinding, TitlebarOptions, WindowBounds, WindowOptions, prelude::*,
    px, size,
};

use app::{
    ContactSheet, ConvertSelected, CreateContactSheet, DeselectPhotos, Dismiss, ExportSheet,
    FocusSearch, ImportFiles, ImportFolder, NewProject, OpenPreview, OpenProject, Quit, Redo,
    RenamePhotos, SaveAs, SaveProject, SelectAllPhotos, ToggleLeftPane, ToggleRightPane, Undo,
};

#[cfg(target_os = "macos")]
use app::{HideOthers, HideSlate};
use photo_preview::{ClosePhotoPreview, NextPhoto, PreviousPhoto};
use preview_window::{ClosePreview, NextPage, PreviousPage};

#[cfg(target_os = "macos")]
const PRIMARY_MODIFIER: &str = "cmd";
#[cfg(not(target_os = "macos"))]
const PRIMARY_MODIFIER: &str = "ctrl";

fn primary(keys: &str) -> String {
    format!("{PRIMARY_MODIFIER}-{keys}")
}

fn main() {
    let _telemetry = telemetry::init();
    let application = Application::new().with_assets(assets::Assets);
    application.on_reopen(|cx| {
        let has_main = cx
            .windows()
            .iter()
            .any(|window| window.downcast::<ContactSheet>().is_some());
        if !has_main {
            let _ = open_main_window(cx, None);
        }
    });
    application.run(|cx: &mut App| {
        assets::install_fonts(cx);
        let bindings = vec![
            KeyBinding::new(&primary("q"), Quit, None),
            KeyBinding::new(&primary("n"), NewProject, None),
            KeyBinding::new(&primary("z"), Undo, None),
            KeyBinding::new(&primary("shift-z"), Redo, None),
            KeyBinding::new(&primary("r"), RenamePhotos, None),
            KeyBinding::new(&primary("shift-p"), OpenPreview, None),
            KeyBinding::new(&primary("o"), OpenProject, None),
            KeyBinding::new(&primary("s"), SaveProject, None),
            KeyBinding::new(&primary("shift-s"), SaveAs, None),
            KeyBinding::new(&primary("e"), ExportSheet, None),
            KeyBinding::new(&primary("shift-c"), CreateContactSheet, None),
            KeyBinding::new(&primary("shift-e"), ConvertSelected, None),
            KeyBinding::new(&primary("i"), ImportFiles, None),
            KeyBinding::new(&primary("shift-i"), ImportFolder, None),
            // Text fields bind the same keys in their own, deeper context, so
            // Cmd/Ctrl+A still selects text while a field is focused.
            KeyBinding::new(&primary("a"), SelectAllPhotos, None),
            KeyBinding::new(&primary("shift-d"), DeselectPhotos, None),
            KeyBinding::new(&primary("k"), FocusSearch, None),
            KeyBinding::new(&primary("f"), FocusSearch, None),
            KeyBinding::new(&primary("b"), ToggleLeftPane, None),
            KeyBinding::new(&primary("alt-b"), ToggleRightPane, None),
            KeyBinding::new("escape", Dismiss, None),
            KeyBinding::new("left", PreviousPage, Some("PreviewWindow")),
            KeyBinding::new("right", NextPage, Some("PreviewWindow")),
            KeyBinding::new("escape", ClosePreview, Some("PreviewWindow")),
            KeyBinding::new("left", PreviousPhoto, Some("PhotoPreviewWindow")),
            KeyBinding::new("right", NextPhoto, Some("PhotoPreviewWindow")),
            KeyBinding::new("escape", ClosePhotoPreview, Some("PhotoPreviewWindow")),
            KeyBinding::new("backspace", text_field::Backspace, Some("TextField")),
            KeyBinding::new("delete", text_field::Delete, Some("TextField")),
            KeyBinding::new("left", text_field::Left, Some("TextField")),
            KeyBinding::new("right", text_field::Right, Some("TextField")),
            KeyBinding::new("shift-left", text_field::SelectLeft, Some("TextField")),
            KeyBinding::new("shift-right", text_field::SelectRight, Some("TextField")),
            KeyBinding::new(&primary("a"), text_field::SelectAll, Some("TextField")),
            KeyBinding::new(&primary("c"), text_field::Copy, Some("TextField")),
            KeyBinding::new(&primary("x"), text_field::Cut, Some("TextField")),
            KeyBinding::new(&primary("v"), text_field::Paste, Some("TextField")),
            KeyBinding::new("home", text_field::Home, Some("TextField")),
            KeyBinding::new("end", text_field::End, Some("TextField")),
            KeyBinding::new("enter", text_field::Submit, Some("TextField")),
        ];
        #[cfg(target_os = "macos")]
        let bindings = {
            let mut bindings = bindings;
            bindings.push(KeyBinding::new(
                "ctrl-cmd-space",
                text_field::ShowCharacterPalette,
                Some("TextField"),
            ));
            bindings.push(KeyBinding::new("cmd-h", HideSlate, None));
            bindings.push(KeyBinding::new("cmd-alt-h", HideOthers, None));
            bindings
        };
        cx.bind_keys(bindings);

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.on_window_closed(|cx| {
            let has_main = cx
                .windows()
                .iter()
                .any(|window| window.downcast::<ContactSheet>().is_some());
            if !has_main {
                cx.quit();
            }
        })
        .detach();

        if let Err(error) = open_main_window(cx, std::env::args().nth(1).map(PathBuf::from)) {
            eprintln!("could not open the Slate window: {error}");
            cx.quit();
        }
    });
}

fn open_main_window(cx: &mut App, path: Option<PathBuf>) -> Result<()> {
    let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(bounds)),
            window_min_size: Some(size(px(1080.0), px(720.0))),
            titlebar: Some(TitlebarOptions {
                title: Some("".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            focus: true,
            ..Default::default()
        },
        |_, cx| {
            let view = cx.new(ContactSheet::new);
            if let Some(path) = path {
                view.update(cx, |this, cx| this.open_project(&path, cx));
            }
            view
        },
    )?;
    cx.activate(true);
    Ok(())
}
