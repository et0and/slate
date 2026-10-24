mod app;
mod chrome;
mod inspector;
mod library;
mod overlay;
mod sidebar;
mod text_field;
mod theme;
mod widgets;

use gpui::{
    App, Application, Bounds, KeyBinding, Menu, MenuItem, SystemMenuType, TitlebarOptions,
    WindowBounds, WindowOptions, prelude::*, px, size,
};

use app::{
    ContactSheet, DeselectPhotos, Dismiss, ExportSheet, ImportFiles, ImportFolder, OpenProject,
    Quit, SaveProject, SelectAllPhotos,
};

fn main() {
    Application::new().run(|cx: &mut App| {
        cx.bind_keys([
            KeyBinding::new("cmd-q", Quit, None),
            KeyBinding::new("cmd-o", OpenProject, None),
            KeyBinding::new("cmd-s", SaveProject, None),
            KeyBinding::new("cmd-e", ExportSheet, None),
            KeyBinding::new("cmd-i", ImportFiles, None),
            KeyBinding::new("cmd-shift-i", ImportFolder, None),
            KeyBinding::new("cmd-shift-a", SelectAllPhotos, None),
            KeyBinding::new("cmd-shift-d", DeselectPhotos, None),
            KeyBinding::new("escape", Dismiss, None),
            KeyBinding::new("backspace", text_field::Backspace, Some("TextField")),
            KeyBinding::new("delete", text_field::Delete, Some("TextField")),
            KeyBinding::new("left", text_field::Left, Some("TextField")),
            KeyBinding::new("right", text_field::Right, Some("TextField")),
            KeyBinding::new("shift-left", text_field::SelectLeft, Some("TextField")),
            KeyBinding::new("shift-right", text_field::SelectRight, Some("TextField")),
            KeyBinding::new("cmd-a", text_field::SelectAll, Some("TextField")),
            KeyBinding::new("cmd-c", text_field::Copy, Some("TextField")),
            KeyBinding::new("cmd-x", text_field::Cut, Some("TextField")),
            KeyBinding::new("cmd-v", text_field::Paste, Some("TextField")),
            KeyBinding::new("home", text_field::Home, Some("TextField")),
            KeyBinding::new("end", text_field::End, Some("TextField")),
            KeyBinding::new("enter", text_field::Submit, Some("TextField")),
            KeyBinding::new(
                "ctrl-cmd-space",
                text_field::ShowCharacterPalette,
                Some("TextField"),
            ),
        ]);

        cx.on_action(|_: &Quit, cx| cx.quit());
        cx.set_menus(vec![
            Menu {
                name: "File".into(),
                items: vec![
                    MenuItem::action("Open Project…", OpenProject),
                    MenuItem::action("Save Project", SaveProject),
                    MenuItem::separator(),
                    MenuItem::action("Export PDF…", ExportSheet),
                    MenuItem::separator(),
                    MenuItem::os_submenu("Services", SystemMenuType::Services),
                    MenuItem::separator(),
                    MenuItem::action("Quit", Quit),
                ],
            },
            Menu {
                name: "Photos".into(),
                items: vec![
                    MenuItem::action("Import Files…", ImportFiles),
                    MenuItem::action("Import Folder…", ImportFolder),
                    MenuItem::separator(),
                    MenuItem::action("Select All Visible", SelectAllPhotos),
                    MenuItem::action("Deselect All", DeselectPhotos),
                ],
            },
        ]);

        let bounds = Bounds::centered(None, size(px(1440.0), px(900.0)), cx);
        cx.open_window(
            WindowOptions {
                window_bounds: Some(WindowBounds::Windowed(bounds)),
                window_min_size: Some(size(px(1080.0), px(720.0))),
                titlebar: Some(TitlebarOptions {
                    title: Some("Contact Sheet".into()),
                    appears_transparent: false,
                    ..Default::default()
                }),
                focus: true,
                ..Default::default()
            },
            |_, cx| {
                let view = cx.new(ContactSheet::new);
                if let Some(path) = std::env::args().nth(1) {
                    view.update(cx, |this, cx| {
                        this.open_project(std::path::PathBuf::from(path), cx)
                    });
                }
                view
            },
        )
        .unwrap();
        cx.activate(true);
    });
}
