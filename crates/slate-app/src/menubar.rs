#![cfg_attr(target_os = "macos", allow(dead_code))]

//! In-window menu bar and its dropdown panels.

use gpui::{
    AnyElement, Context, ElementId, Menu, MenuItem, MouseButton, Stateful, SystemMenuType, div,
    prelude::*, px,
};

use crate::app::{
    About, ContactSheet, DeselectPhotos, ExportDrive, ExportSheet, HideOthers, HideSlate,
    ImportDrive, ImportFiles, ImportFolder, InvertSelection, LinkDrive, NewProject, OpenPreview,
    OpenProject, OpenRecent, OpenWebsite, Quit, Redo, RemoveSelected, ReportIssue, ResetPanels,
    SaveAs, SaveProject, SaveSheet, SelectAllPhotos, SetTheme, Undo, UnlinkDrive,
};
use crate::assets;
use crate::drive_ui::DriveMode;
use crate::theme;
use crate::widgets::{icon, mono_text, shift_shortcut_label, shortcut_label, ui_text};

const MENU_START: f32 = 8.0;
const MENU_TEXT_INSET: f32 = 8.0;
const MENU_FILE_W: f32 = 36.0;
const MENU_EDIT_W: f32 = 36.0;
const MENU_VIEW_W: f32 = 40.0;
const MENU_DRIVE_W: f32 = 42.0;
const MENU_HISTORY_W: f32 = 56.0;
const MENU_HELP_W: f32 = 38.0;
const PANEL_W: f32 = 280.0;
const ITEM_H: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MenuId {
    File,
    Edit,
    View,
    Drive,
    History,
    Help,
}

impl MenuId {
    fn index(self) -> usize {
        match self {
            Self::File => 0,
            Self::Edit => 1,
            Self::View => 2,
            Self::Drive => 3,
            Self::History => 4,
            Self::Help => 5,
        }
    }

    fn width(self) -> f32 {
        match self {
            Self::File => MENU_FILE_W,
            Self::Edit => MENU_EDIT_W,
            Self::View => MENU_VIEW_W,
            Self::Drive => MENU_DRIVE_W,
            Self::History => MENU_HISTORY_W,
            Self::Help => MENU_HELP_W,
        }
    }

    fn left(self) -> f32 {
        MENU_START
            + match self {
                Self::File => 0.0,
                Self::Edit => MENU_FILE_W,
                Self::View => MENU_FILE_W + MENU_EDIT_W,
                Self::Drive => MENU_FILE_W + MENU_EDIT_W + MENU_VIEW_W,
                Self::History => MENU_FILE_W + MENU_EDIT_W + MENU_VIEW_W + MENU_DRIVE_W,
                Self::Help => {
                    MENU_FILE_W + MENU_EDIT_W + MENU_VIEW_W + MENU_DRIVE_W + MENU_HISTORY_W
                }
            }
    }
}

impl ContactSheet {
    pub(crate) fn render_menubar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex()
            .items_center()
            .child(self.menu_trigger(MenuId::File, "File", cx))
            .child(self.menu_trigger(MenuId::Edit, "Edit", cx))
            .child(self.menu_trigger(MenuId::View, "View", cx))
            .child(self.menu_trigger(MenuId::Drive, "Drive", cx))
            .child(self.menu_trigger(MenuId::History, "History", cx))
            .child(self.menu_trigger(MenuId::Help, "Help", cx))
            .into_any_element()
    }

    fn menu_trigger(
        &self,
        id: MenuId,
        label: &'static str,
        cx: &mut Context<Self>,
    ) -> Stateful<gpui::Div> {
        let active = self.menu_open == Some(id);
        let trigger = div()
            .id(("menu-trigger", id.index()))
            .w(px(id.width()))
            .h(px(theme::TOPBAR_H))
            .flex()
            .items_center()
            .justify_center()
            .cursor_pointer()
            .font_family(theme::FONT_UI)
            .font_weight(gpui::FontWeight::MEDIUM)
            .text_size(px(theme::TEXT_BODY))
            .text_color(if active { theme::ink() } else { theme::muted() })
            .active(|style| style.opacity(0.72))
            .on_click(cx.listener(move |this, _, _, cx| this.toggle_menu(id, cx)));
        if active {
            trigger.bg(theme::accent_soft()).child(label)
        } else {
            trigger
                .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
                .child(label)
        }
    }

    fn toggle_menu(&mut self, id: MenuId, cx: &mut Context<Self>) {
        self.menu_open = if self.menu_open == Some(id) {
            None
        } else {
            Some(id)
        };
        cx.notify();
    }

    pub(crate) fn close_menu(&mut self, cx: &mut Context<Self>) {
        if self.menu_open.take().is_some() {
            cx.notify();
        }
    }

    pub(crate) fn render_menu(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        let id = self.menu_open?;
        let left = id.left() + MENU_TEXT_INSET;
        let rows: Vec<AnyElement> = match id {
            MenuId::File => self.file_menu(cx),
            MenuId::Edit => self.edit_menu(cx),
            MenuId::View => self.view_menu(cx),
            MenuId::Drive => self.drive_menu(cx),
            MenuId::History => self.history_menu(),
            MenuId::Help => self.help_menu(cx),
        };

        Some(
            div()
                .absolute()
                .top(px(theme::TOPBAR_H))
                .left(px(0.0))
                .right(px(0.0))
                .bottom(px(0.0))
                .child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .left(px(0.0))
                        .right(px(0.0))
                        .bottom(px(0.0))
                        .on_mouse_down(
                            MouseButton::Left,
                            cx.listener(|this, _, _, cx| this.close_menu(cx)),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(0.0))
                        .left(px(left))
                        .w(px(PANEL_W))
                        .py(px(4.0))
                        .bg(theme::surface())
                        .border_1()
                        .border_color(theme::rule())
                        .shadow_lg()
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    fn file_menu(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = vec![
            menu_item(
                "menu-new",
                assets::FILE_PLUS,
                "New project",
                Some(shortcut_label("N")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.new_project(cx);
            }))
            .into_any_element(),
            menu_item(
                "menu-open",
                assets::FOLDER,
                "Open project…",
                Some(shortcut_label("O")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.open_project_prompt(cx);
            }))
            .into_any_element(),
        ];

        if !self.recent_projects.is_empty() {
            rows.push(separator());
            rows.push(section_label("Recent"));
            for (index, recent) in self.recent_projects.iter().take(4).enumerate() {
                let name = recent.name.clone();
                let path = recent.path.clone();
                rows.push(
                    menu_item("menu-recent", assets::CLOCK, name, None, true)
                        .id(ElementId::Name(gpui::SharedString::from(format!(
                            "menu-recent-{index}"
                        ))))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_menu(cx);
                            this.open_project(path.clone(), cx);
                        }))
                        .into_any_element(),
                );
            }
        }

        rows.push(separator());
        rows.push(
            menu_item(
                "menu-save",
                assets::SAVE,
                "Save",
                Some(shortcut_label("S")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.save_project(cx);
            }))
            .into_any_element(),
        );
        rows.push(
            menu_item(
                "menu-save-as",
                assets::SAVE,
                "Save as…",
                Some(shift_shortcut_label("S")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.save_project_as(cx);
            }))
            .into_any_element(),
        );
        rows.push(
            menu_item("menu-save-sheet", assets::SAVE, "Save sheet", None, true)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.close_menu(cx);
                    this.save_sheet(cx);
                }))
                .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            menu_item(
                "menu-import-files",
                assets::FILE_PLUS,
                "Import files…",
                Some(shortcut_label("I")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.import_prompt(false, cx);
            }))
            .into_any_element(),
        );
        rows.push(
            menu_item(
                "menu-import-folder",
                assets::FOLDER_PLUS,
                "Import folder…",
                Some(shift_shortcut_label("I")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.import_prompt(true, cx);
            }))
            .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            menu_item(
                "menu-export",
                assets::DOWNLOAD,
                "Export PDF…",
                Some(shortcut_label("E")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.export_current(cx);
            }))
            .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            menu_item(
                "menu-quit",
                assets::X,
                "Quit",
                Some(shortcut_label("Q")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                cx.quit();
            }))
            .into_any_element(),
        );
        rows
    }

    fn edit_menu(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let can_undo = self.history.can_undo();
        let can_redo = self.history.can_redo();

        let mut rows = vec![
            menu_item(
                "menu-undo",
                assets::UNDO,
                "Undo",
                Some(shortcut_label("Z")),
                can_undo,
            )
            .when(can_undo, |item| {
                item.on_click(cx.listener(|this, _, _, cx| {
                    this.close_menu(cx);
                    this.undo(cx);
                }))
            })
            .into_any_element(),
            menu_item(
                "menu-redo",
                assets::REDO,
                "Redo",
                Some(shift_shortcut_label("Z")),
                can_redo,
            )
            .when(can_redo, |item| {
                item.on_click(cx.listener(|this, _, _, cx| {
                    this.close_menu(cx);
                    this.redo(cx);
                }))
            })
            .into_any_element(),
            separator(),
            menu_item(
                "menu-select-all",
                assets::CHECK,
                "Select all visible",
                Some(shift_shortcut_label("A")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.select_all_visible(cx);
            }))
            .into_any_element(),
            menu_item(
                "menu-deselect-all",
                assets::X,
                "Deselect all",
                Some(shift_shortcut_label("D")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.deselect_all(cx);
            }))
            .into_any_element(),
            menu_item("menu-invert", assets::CHECK, "Invert selection", None, true)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.close_menu(cx);
                    this.invert_visible(cx);
                }))
                .into_any_element(),
            separator(),
            menu_item(
                "menu-remove",
                assets::TRASH,
                "Remove from project",
                None,
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.remove_selected(cx);
            }))
            .into_any_element(),
        ];
        rows.shrink_to_fit();
        rows
    }

    fn view_menu(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows: Vec<AnyElement> = vec![
            menu_item(
                "menu-preview",
                assets::IMAGE,
                "Open preview window",
                Some(shift_shortcut_label("P")),
                true,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_menu(cx);
                this.open_preview_window(window, cx);
            }))
            .into_any_element(),
            separator(),
        ];
        rows.extend(
            theme::Theme::ALL
                .into_iter()
                .map(|choice| self.theme_menu_item(choice, cx)),
        );
        rows.push(separator());
        rows.push(
            menu_item(
                "menu-reset-panels",
                assets::LAYOUT,
                "Reset panel widths",
                None,
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.panels.reset();
                cx.notify();
            }))
            .into_any_element(),
        );
        rows
    }

    fn theme_menu_item(&self, choice: theme::Theme, cx: &mut Context<Self>) -> AnyElement {
        let active = self.theme == choice;
        let check = div()
            .w(px(14.0))
            .h(px(14.0))
            .flex()
            .items_center()
            .justify_center()
            .children(active.then(|| icon(assets::CHECK, 13.0, theme::accent())));
        div()
            .id(choice.label())
            .w_full()
            .h(px(ITEM_H))
            .px(px(10.0))
            .flex()
            .items_center()
            .gap(px(9.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme::accent_soft()))
            .active(|style| style.opacity(0.8))
            .on_click(cx.listener(move |this, _, _, cx| {
                this.close_menu(cx);
                this.set_theme(choice, cx);
            }))
            .child(check)
            .child(
                ui_text(theme::TEXT_BODY, theme::ink())
                    .font_weight(if active {
                        gpui::FontWeight::MEDIUM
                    } else {
                        gpui::FontWeight::NORMAL
                    })
                    .child(choice.label()),
            )
            .into_any_element()
    }

    fn drive_menu(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let mut rows = Vec::new();
        let Some(account) = self.drive_account.clone() else {
            rows.push(section_label("Google Drive"));
            rows.push(
                menu_item(
                    "drive-link",
                    assets::EXTERNAL_LINK,
                    if self.drive_linking {
                        "Waiting for browser…"
                    } else {
                        "Link Google Drive…"
                    },
                    None,
                    !self.drive_linking,
                )
                .when(!self.drive_linking, |item| {
                    item.on_click(cx.listener(|this, _, _, cx| {
                        this.link_google_drive(cx);
                    }))
                })
                .into_any_element(),
            );
            return rows;
        };

        let can_use =
            !self.drive_linking && self.drive_download.is_none() && self.exporting.is_none();
        rows.push(section_label("Google Drive"));
        rows.push(
            div()
                .h(px(24.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .child(
                    mono_text(theme::TEXT_META, theme::faint())
                        .truncate()
                        .child(account.label()),
                )
                .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            menu_item(
                "drive-import",
                assets::FOLDER_PLUS,
                "Import from Drive…",
                None,
                can_use,
            )
            .when(can_use, |item| {
                item.on_click(cx.listener(|this, _, _, cx| {
                    this.open_drive_picker(DriveMode::Import, cx);
                }))
            })
            .into_any_element(),
        );
        rows.push(
            menu_item(
                "drive-export",
                assets::UPLOAD,
                "Export sheet to Drive…",
                None,
                can_use,
            )
            .when(can_use, |item| {
                item.on_click(cx.listener(|this, _, _, cx| {
                    this.open_drive_picker(DriveMode::Export, cx);
                }))
            })
            .into_any_element(),
        );
        let can_unlink =
            !self.drive_linking && self.drive_download.is_none() && self.exporting.is_none();
        rows.push(separator());
        rows.push(
            menu_item(
                "drive-unlink",
                assets::X,
                "Unlink Google Drive",
                None,
                can_unlink,
            )
            .when(can_unlink, |item| {
                item.on_click(cx.listener(|this, _, _, cx| {
                    this.unlink_google_drive(cx);
                }))
            })
            .into_any_element(),
        );
        rows
    }

    fn history_menu(&mut self) -> Vec<AnyElement> {
        let entries: Vec<String> = self.log.iter().rev().take(8).cloned().collect();
        if entries.is_empty() {
            return vec![
                div()
                    .h(px(ITEM_H))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .child(mono_text(theme::TEXT_META, theme::faint()).child("No history"))
                    .into_any_element(),
            ];
        }
        entries
            .into_iter()
            .map(|line| {
                div()
                    .h(px(26.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(icon(assets::CLOCK, 11.0, theme::faint()))
                    .child(
                        mono_text(theme::TEXT_MICRO, theme::muted())
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .child(line),
                    )
                    .into_any_element()
            })
            .collect()
    }

    fn help_menu(&mut self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        vec![
            menu_item("menu-about", assets::INFO, "About", None, true)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.menu_open = None;
                    this.open_about_window(cx);
                }))
                .into_any_element(),
        ]
    }
}

impl ContactSheet {
    #[cfg(target_os = "macos")]
    pub(crate) fn sync_native_menus(&self, cx: &mut Context<Self>) {
        cx.set_menus(self.native_menus());
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn sync_native_menus(&self, _cx: &mut Context<Self>) {}

    #[cfg(target_os = "macos")]
    fn native_menus(&self) -> Vec<Menu> {
        let app_menu = Menu {
            name: "Slate".into(),
            items: vec![
                MenuItem::action("About Slate", About),
                MenuItem::separator(),
                MenuItem::action("Hide Slate", HideSlate),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action("Quit Slate", Quit),
            ],
        };

        let mut file_items = vec![
            MenuItem::action("New project", NewProject),
            MenuItem::action("Open project…", OpenProject),
        ];
        if !self.recent_projects.is_empty() {
            file_items.push(MenuItem::submenu(Menu {
                name: "Recent".into(),
                items: self
                    .recent_projects
                    .iter()
                    .take(4)
                    .map(|recent| {
                        MenuItem::action(
                            recent.name.clone(),
                            OpenRecent {
                                path: recent.path.clone(),
                            },
                        )
                    })
                    .collect(),
            }));
        }
        file_items.extend([
            MenuItem::separator(),
            MenuItem::action("Save", SaveProject),
            MenuItem::action("Save as…", SaveAs),
            MenuItem::action("Save sheet", SaveSheet),
            MenuItem::separator(),
            MenuItem::action("Import files…", ImportFiles),
            MenuItem::action("Import folder…", ImportFolder),
            MenuItem::separator(),
            MenuItem::action("Export PDF…", ExportSheet),
        ]);

        let edit_items = vec![
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
            MenuItem::separator(),
            MenuItem::action("Select all visible", SelectAllPhotos),
            MenuItem::action("Deselect all", DeselectPhotos),
            MenuItem::action("Invert selection", InvertSelection),
            MenuItem::separator(),
            MenuItem::action("Remove from project", RemoveSelected),
        ];

        let mut view_items = vec![
            MenuItem::action("Open preview window", OpenPreview),
            MenuItem::separator(),
        ];
        view_items.extend(theme::Theme::ALL.into_iter().map(|choice| {
            let label = if self.theme == choice {
                format!("✓ {}", choice.label())
            } else {
                choice.label().to_string()
            };
            MenuItem::action(label, SetTheme { theme: choice })
        }));
        view_items.push(MenuItem::separator());
        view_items.push(MenuItem::action("Reset panel widths", ResetPanels));

        let drive_items = if self.drive_account.is_some() {
            vec![
                MenuItem::action("Import from Drive…", ImportDrive),
                MenuItem::action("Export sheet to Drive…", ExportDrive),
                MenuItem::separator(),
                MenuItem::action("Unlink Google Drive", UnlinkDrive),
            ]
        } else {
            vec![MenuItem::action("Link Google Drive…", LinkDrive)]
        };

        let help_items = vec![
            MenuItem::action("Slate website", OpenWebsite),
            MenuItem::action("Report an issue", ReportIssue),
        ];

        vec![
            app_menu,
            Menu {
                name: "File".into(),
                items: file_items,
            },
            Menu {
                name: "Edit".into(),
                items: edit_items,
            },
            Menu {
                name: "View".into(),
                items: view_items,
            },
            Menu {
                name: "Drive".into(),
                items: drive_items,
            },
            Menu {
                name: "Help".into(),
                items: help_items,
            },
        ]
    }
}

fn section_label(label: &'static str) -> AnyElement {
    div()
        .h(px(24.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .font_family(theme::FONT_UI)
        .text_size(px(theme::TEXT_META))
        .text_color(theme::faint())
        .child(label)
        .into_any_element()
}

fn menu_item(
    id: impl Into<ElementId>,
    icon_path: &'static str,
    label: impl Into<gpui::SharedString>,
    hint: Option<String>,
    enabled: bool,
) -> Stateful<gpui::Div> {
    let label = label.into();
    let color = if enabled {
        theme::muted()
    } else {
        theme::faint()
    };
    let item = div()
        .id(id)
        .w_full()
        .h(px(ITEM_H))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .child(icon(icon_path, 14.0, color))
        .child(
            div()
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .font_family(theme::FONT_UI)
                .font_weight(if enabled {
                    gpui::FontWeight::MEDIUM
                } else {
                    gpui::FontWeight::NORMAL
                })
                .text_size(px(theme::TEXT_BODY))
                .text_color(if enabled {
                    theme::ink()
                } else {
                    theme::faint()
                })
                .child(label),
        )
        .children(hint.map(|hint| mono_text(theme::TEXT_META, theme::faint()).child(hint)));
    if enabled {
        item.cursor_pointer()
            .hover(|style| style.bg(theme::accent_soft()))
            .active(|style| style.opacity(0.8))
    } else {
        item
    }
}

fn separator() -> AnyElement {
    div()
        .h(px(1.0))
        .w_full()
        .bg(theme::rule())
        .into_any_element()
}
