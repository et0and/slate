#![cfg_attr(target_os = "macos", allow(dead_code))]

//! In-window menu bar and its dropdown panels.

use gpui::{AnyElement, Context, ElementId, MouseButton, Stateful, div, prelude::*, px};

#[cfg(target_os = "macos")]
use gpui::{Menu, MenuItem, SystemMenuType};

use crate::app::ContactSheet;

#[cfg(target_os = "macos")]
use crate::app::{
    About, ConvertSelected, CreateContactSheet, DeselectPhotos, ExportDrive, ExportSheet,
    FocusSearch, HideOthers, HideSlate, ImportDrive, ImportFiles, ImportFolder, ImportVideo,
    InvertSelection, NewProject, OpenPreview, OpenProject, OpenRecent, Quit, Redo, RemoveSelected,
    RenamePhotos, ResetPanels, SaveAs, SaveProject, SaveSheet, SelectAllPhotos, SetTheme,
    ToggleLeftPane, ToggleRightPane, Undo, UnlinkDrive,
};
use crate::app::{OpenWebsite, ReportIssue};
use crate::assets;
use crate::drive_ui::DriveMode;
use crate::theme;
use crate::widgets::{
    alt_shortcut_label, icon, mono_text, shift_shortcut_label, shortcut_hint, shortcut_label,
    ui_text,
};

const MENU_START: f32 = 8.0;
const MENU_TEXT_INSET: f32 = 8.0;
const MENU_FILE_W: f32 = 36.0;
const MENU_EDIT_W: f32 = 36.0;
const MENU_VIEW_W: f32 = 40.0;
const MENU_HISTORY_W: f32 = 56.0;
const MENU_HELP_W: f32 = 38.0;
const PANEL_W: f32 = 280.0;
const ITEM_H: f32 = 30.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuId {
    File,
    Edit,
    View,
    History,
    Help,
}

impl MenuId {
    const fn index(self) -> usize {
        match self {
            Self::File => 0,
            Self::Edit => 1,
            Self::View => 2,
            Self::History => 3,
            Self::Help => 4,
        }
    }

    const fn width(self) -> f32 {
        match self {
            Self::File => MENU_FILE_W,
            Self::Edit => MENU_EDIT_W,
            Self::View => MENU_VIEW_W,
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
                Self::History => MENU_FILE_W + MENU_EDIT_W + MENU_VIEW_W,
                Self::Help => MENU_FILE_W + MENU_EDIT_W + MENU_VIEW_W + MENU_HISTORY_W,
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
                    // Occlude the dismiss layer, or a press on an item closes
                    // the menu before the item can handle its click.
                    div()
                        .absolute()
                        .top(px(0.0))
                        .left(px(left))
                        .w(px(PANEL_W))
                        .occlude()
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
                "New collection",
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
                "Open folder",
                Some(shortcut_label("O")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.open_project_prompt(cx);
            }))
            .into_any_element(),
        ];

        if !self.recent_collections.is_empty() {
            rows.push(separator());
            rows.push(section_label("Recent folders"));
            for (index, recent) in self.recent_collections.iter().take(4).enumerate() {
                let name = recent.name.clone();
                let path = recent.path.clone();
                rows.push(
                    menu_item("menu-recent", assets::CLOCK, name, None, true)
                        .id(ElementId::Name(gpui::SharedString::from(format!(
                            "menu-recent-{index}"
                        ))))
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.close_menu(cx);
                            this.open_project(&path, cx);
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
                "Save collection",
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
                "Save collection as",
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
        rows.push(
            menu_item(
                "menu-create-sheet",
                assets::LAYOUT,
                "Create contact sheet",
                Some(shift_shortcut_label("C")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.open_sheet_builder(cx);
            }))
            .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            menu_item(
                "menu-import-files",
                assets::FILE_PLUS,
                "Import files",
                Some(shortcut_label("I")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.import_files_prompt(cx);
            }))
            .into_any_element(),
        );
        rows.push(
            menu_item(
                "menu-import-folder",
                assets::FOLDER_PLUS,
                "Import folder",
                Some(shift_shortcut_label("I")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.import_folder_prompt(cx);
            }))
            .into_any_element(),
        );
        rows.push(
            menu_item(
                "menu-import-video",
                assets::FILE_PLUS,
                "Import video",
                None,
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.import_video_prompt(cx);
            }))
            .into_any_element(),
        );
        rows.push(self.drive_menu_item(DriveMode::Import, cx));
        rows.push(separator());
        rows.push(
            menu_item(
                "menu-convert",
                assets::UPLOAD,
                "Convert selected",
                Some(shift_shortcut_label("E")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.open_convert_dialog(cx);
            }))
            .into_any_element(),
        );
        rows.push(
            menu_item(
                "menu-export",
                assets::DOWNLOAD,
                "Export PDF",
                Some(shortcut_label("E")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.export_current(cx);
            }))
            .into_any_element(),
        );
        rows.push(self.drive_menu_item(DriveMode::Export, cx));
        if self.drive_account.is_some() {
            let can_unlink = self.drive_idle();
            rows.push(separator());
            rows.push(
                menu_item(
                    "menu-drive-unlink",
                    assets::X,
                    "Unlink Google Drive",
                    None,
                    can_unlink,
                )
                .when(can_unlink, |item| {
                    item.on_click(cx.listener(|this, _, _, cx| this.unlink_google_drive(cx)))
                })
                .into_any_element(),
            );
        }
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
                "menu-search",
                assets::SEARCH,
                "Search",
                Some(shortcut_label("K")),
                true,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_menu(cx);
                this.focus_search(window, cx);
            }))
            .into_any_element(),
            menu_item(
                "menu-select-all",
                assets::CHECK,
                "Select all visible",
                Some(shortcut_label("A")),
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
                "menu-rename-photos",
                assets::PENCIL,
                "Rename",
                Some(shortcut_label("R")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.open_rename_window(cx);
            }))
            .into_any_element(),
            separator(),
            menu_item(
                "menu-remove",
                assets::TRASH,
                "Remove from collection",
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
                "Open sheet preview",
                Some(shift_shortcut_label("P")),
                true,
            )
            .on_click(cx.listener(|this, _, _, cx| {
                this.close_menu(cx);
                this.open_preview_window(cx);
            }))
            .into_any_element(),
            separator(),
            menu_item(
                "menu-toggle-left",
                assets::PANEL_LEFT,
                "Toggle left pane",
                Some(shortcut_label("B")),
                true,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_menu(cx);
                this.toggle_left_pane(window, cx);
            }))
            .into_any_element(),
            menu_item(
                "menu-toggle-right",
                assets::PANEL_RIGHT,
                "Toggle right pane",
                Some(alt_shortcut_label("B")),
                true,
            )
            .on_click(cx.listener(|this, _, window, cx| {
                this.close_menu(cx);
                this.toggle_right_pane(window, cx);
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
            .child(ui_text(theme::TEXT_BODY, theme::ink()).child(choice.label()))
            .into_any_element()
    }

    /// Drive work runs one transfer at a time.
    const fn drive_idle(&self) -> bool {
        !self.drive_linking && self.drive_download.is_none() && self.exporting.is_none()
    }

    /// Import and export open the Drive browser, linking the account first.
    fn drive_menu_item(&self, mode: DriveMode, cx: &mut Context<Self>) -> AnyElement {
        let enabled = self.drive_idle();
        let (id, icon_path, label) = match mode {
            DriveMode::Import => (
                "menu-drive-import",
                assets::CLOUD,
                "Import from Google Drive",
            ),
            DriveMode::Export => (
                "menu-drive-export",
                assets::UPLOAD,
                "Export PDF to Google Drive",
            ),
        };
        menu_item(id, icon_path, label, None, enabled)
            .when(enabled, |item| {
                item.on_click(cx.listener(move |this, _, _, cx| {
                    this.close_menu(cx);
                    this.open_drive(mode, cx);
                }))
            })
            .into_any_element()
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
                        mono_text(theme::TEXT_META, theme::muted())
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
            menu_item(
                "menu-website",
                assets::EXTERNAL_LINK,
                "Slate website",
                None,
                true,
            )
            .on_click(cx.listener(|_, _, window, cx| {
                window.dispatch_action(Box::new(OpenWebsite), cx);
            }))
            .into_any_element(),
            menu_item("menu-report", assets::ALERT, "Report an issue", None, true)
                .on_click(cx.listener(|_, _, window, cx| {
                    window.dispatch_action(Box::new(ReportIssue), cx);
                }))
                .into_any_element(),
            separator(),
            menu_item("menu-about", assets::INFO, "About Slate", None, true)
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
    pub(crate) const fn sync_native_menus(&self, _cx: &mut Context<Self>) {}

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
            MenuItem::action("New collection", NewProject),
            MenuItem::action("Open folder", OpenProject),
        ];
        if !self.recent_collections.is_empty() {
            file_items.push(MenuItem::submenu(Menu {
                name: "Recent folders".into(),
                items: self
                    .recent_collections
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
            MenuItem::action("Save collection", SaveProject),
            MenuItem::action("Save collection as", SaveAs),
            MenuItem::action("Save sheet", SaveSheet),
            MenuItem::action("Create contact sheet", CreateContactSheet),
            MenuItem::separator(),
            MenuItem::action("Import files", ImportFiles),
            MenuItem::action("Import folder", ImportFolder),
            MenuItem::action("Import video", ImportVideo),
            MenuItem::action("Import from Google Drive", ImportDrive),
            MenuItem::separator(),
            MenuItem::action("Convert selected", ConvertSelected),
            MenuItem::action("Export PDF", ExportSheet),
            MenuItem::action("Export PDF to Google Drive", ExportDrive),
        ]);
        if self.drive_account.is_some() {
            file_items.extend([
                MenuItem::separator(),
                MenuItem::action("Unlink Google Drive", UnlinkDrive),
            ]);
        }

        let edit_items = vec![
            MenuItem::action("Undo", Undo),
            MenuItem::action("Redo", Redo),
            MenuItem::separator(),
            MenuItem::action("Search", FocusSearch),
            MenuItem::action("Select all visible", SelectAllPhotos),
            MenuItem::action("Deselect all", DeselectPhotos),
            MenuItem::action("Invert selection", InvertSelection),
            MenuItem::separator(),
            MenuItem::action("Rename", RenamePhotos),
            MenuItem::separator(),
            MenuItem::action("Remove from collection", RemoveSelected),
        ];

        let mut view_items = vec![
            MenuItem::action("Open sheet preview", OpenPreview),
            MenuItem::separator(),
            MenuItem::action("Toggle left pane", ToggleLeftPane),
            MenuItem::action("Toggle right pane", ToggleRightPane),
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
                .text_size(px(theme::TEXT_BODY))
                .text_color(if enabled {
                    theme::ink()
                } else {
                    theme::faint()
                })
                .child(label),
        )
        .children(hint.map(shortcut_hint));
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
