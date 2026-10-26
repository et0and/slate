//! In-window dialogs and the per-photo context menu.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, prelude::*, px};

use slate_core::convert::OutputFormat;

use crate::app::ContactSheet;
use crate::assets;
use crate::drive;
use crate::drive_ui::DriveMode;
use crate::theme;
use crate::widgets::{
    button, dialog_footer, dialog_header, dialog_panel, disabled_button, group_label, icon,
    icon_button, micro, modal, primary_button, ui_text,
};

const CONTEXT_W: f32 = 224.0;
const CONTEXT_ROW_H: f32 = 36.0;

impl ContactSheet {
    pub(crate) fn render_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(menu) = self.render_photo_context(window, cx) {
            return Some(menu);
        }
        if let Some(menu) = self.render_folder_menu(window, cx) {
            return Some(menu);
        }
        if self.rename_photo_target.is_some() {
            return Some(self.render_photo_rename(cx));
        }
        if self.convert_dialog_open {
            return Some(self.render_convert_dialog(cx));
        }
        if let Some(notice) = self.render_drive_notice(cx) {
            return Some(notice);
        }
        if let Some(picker) = self.render_drive_picker(cx) {
            return Some(picker);
        }
        None
    }

    fn render_convert_dialog(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let count = self.scope_ids().len();
        let format = self.convert_format;
        let panel = dialog_panel(440.0)
            .debug_selector(|| "convert-dialog".into())
            .child(dialog_header(
                "Convert photos",
                Some(format!("{count} photo(s) in scope / copies only")),
                icon_button("convert-close", assets::X)
                    .on_click(cx.listener(|this, _, _, cx| this.close_convert_dialog(cx))),
            ))
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(group_label("Format"))
                    .child(div().grid().grid_cols(2).gap(px(6.0)).children(
                        OutputFormat::ALL.into_iter().map(|choice| {
                            let active = choice == format;
                            div()
                                .id(ElementId::Name(SharedString::from(format!(
                                    "convert-format-{}",
                                    choice.extension()
                                ))))
                                .h(px(38.0))
                                .flex()
                                .items_center()
                                .justify_center()
                                .border_1()
                                .border_color(if active { theme::ink() } else { theme::rule() })
                                .bg(if active {
                                    theme::accent_soft()
                                } else {
                                    theme::surface()
                                })
                                .cursor_pointer()
                                .hover(|style| style.bg(theme::accent_soft()))
                                .font_family(theme::FONT_UI)
                                .font_weight(gpui::FontWeight::MEDIUM)
                                .text_size(px(theme::TEXT_BODY))
                                .text_color(if active { theme::ink() } else { theme::muted() })
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.set_convert_format(choice, cx);
                                }))
                                .child(choice.label())
                                .into_any_element()
                        }),
                    )),
            )
            .child(
                dialog_footer()
                    .child(
                        button("convert-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.close_convert_dialog(cx))),
                    )
                    .child(if count == 0 {
                        disabled_button("Choose folder").into_any_element()
                    } else {
                        primary_button("convert-start", "Choose folder")
                            .on_click(cx.listener(|this, _, _, cx| this.start_conversion(cx)))
                            .into_any_element()
                    }),
            );
        modal(
            panel,
            cx.listener(|this, _, _, cx| this.close_convert_dialog(cx)),
        )
    }

    fn render_photo_context(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.photo_context.clone()?;
        let viewport = window.viewport_size();
        let height = 5.0f32.mul_add(CONTEXT_ROW_H, 2.0) + 8.0;
        let left = f32::from(menu.position.x)
            .min(f32::from(viewport.width) - CONTEXT_W - 8.0)
            .max(8.0);
        let top = f32::from(menu.position.y)
            .min(f32::from(viewport.height) - height - 8.0)
            .max(8.0);
        let photo_id = menu.photo_id;
        #[cfg(target_os = "macos")]
        let reveal_label = "Reveal in Finder";
        #[cfg(not(target_os = "macos"))]
        let reveal_label = "Open file";

        let separator = || div().h(px(1.0)).bg(theme::rule()).into_any_element();
        let rows: Vec<AnyElement> = vec![
            context_item("photo-context-rename", assets::PENCIL, "Rename")
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_photo_rename(photo_id, window, cx);
                }))
                .into_any_element(),
            context_item("photo-context-info", assets::INFO, "Show in inspector")
                .on_click(cx.listener(move |this, _, _, cx| this.inspect_photo(photo_id, cx)))
                .into_any_element(),
            separator(),
            context_item("photo-context-reveal", assets::FOLDER, reveal_label)
                .on_click(cx.listener(move |this, _, _, cx| this.reveal_photo(photo_id, cx)))
                .into_any_element(),
            context_item("photo-context-copy", assets::COPY, "Copy file path")
                .on_click(cx.listener(move |this, _, _, cx| this.copy_photo_path(photo_id, cx)))
                .into_any_element(),
            separator(),
            context_item(
                "photo-context-remove",
                assets::TRASH,
                "Remove from collection",
            )
            .on_click(cx.listener(move |this, _, _, cx| this.remove_photo(photo_id, cx)))
            .into_any_element(),
        ];

        Some(floating_menu(
            left,
            top,
            rows,
            cx.listener(|this, _, _, cx| {
                this.photo_context = None;
                cx.notify();
            }),
        ))
    }

    /// The Folders "+" menu: add a local folder set, or a Google Drive folder
    /// that is mirrored locally and joins the tree.
    fn render_folder_menu(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let position = self.folder_menu?;
        let viewport = window.viewport_size();
        let height = 2.0f32.mul_add(CONTEXT_ROW_H, 8.0) + 2.0;
        let left = f32::from(position.x)
            .min(f32::from(viewport.width) - CONTEXT_W - 8.0)
            .max(8.0);
        let top = (f32::from(position.y) + 8.0)
            .min(f32::from(viewport.height) - height - 8.0)
            .max(8.0);
        let drive_meta = if self.drive_linking {
            "Waiting…".to_string()
        } else {
            self.drive_account
                .as_ref()
                .map_or_else(|| "Not linked".to_string(), drive::DriveAccount::label)
        };
        let rows = vec![
            context_item("folder-menu-local", assets::FOLDER_PLUS, "Local folder")
                .on_click(cx.listener(|this, _, _, cx| {
                    this.folder_menu = None;
                    this.open_project_prompt(cx);
                }))
                .into_any_element(),
            context_item("folder-menu-drive", assets::CLOUD, "Google Drive")
                .child(micro(drive_meta).min_w(px(0.0)).truncate())
                .on_click(cx.listener(|this, _, _, cx| {
                    this.folder_menu = None;
                    this.open_drive(DriveMode::Import, cx);
                }))
                .into_any_element(),
        ];
        Some(floating_menu(
            left,
            top,
            rows,
            cx.listener(|this, _, _, cx| {
                this.folder_menu = None;
                cx.notify();
            }),
        ))
    }

    fn render_photo_rename(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .rename_photo_target
            .and_then(|id| self.project.photo(id))
            .map(slate_core::Photo::file_name)
            .unwrap_or_default();
        let error = self.rename_photo_error.clone();
        let panel = dialog_panel(460.0)
            .child(dialog_header(
                "Rename photo",
                Some(current),
                icon_button("photo-rename-close", assets::X)
                    .on_click(cx.listener(|this, _, _, cx| this.cancel_photo_rename(cx))),
            ))
            .child(
                div()
                    .px(px(20.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.rename_photo_field.clone())
                    .children(error.map(|error| {
                        ui_text(theme::TEXT_META, theme::muted())
                            .line_height(px(16.0))
                            .child(error)
                    })),
            )
            .child(
                dialog_footer()
                    .justify_end()
                    .child(
                        button("photo-rename-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_photo_rename(cx))),
                    )
                    .child(
                        primary_button("photo-rename-apply", "Rename").on_click(cx.listener(
                            |this, _, _, cx| {
                                let value = this.rename_photo_field.read(cx).value().to_string();
                                this.apply_photo_rename(&value, cx);
                            },
                        )),
                    ),
            );
        modal(
            panel,
            cx.listener(|this, _, _, cx| this.cancel_photo_rename(cx)),
        )
    }
}

/// A menu floating at a point over a full-window dismiss layer. The panel
/// occludes that layer so a press on a row reaches the row instead of closing
/// the menu first.
fn floating_menu(
    left: f32,
    top: f32,
    rows: Vec<AnyElement>,
    on_dismiss: impl Fn(&gpui::MouseDownEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .on_mouse_down(gpui::MouseButton::Left, on_dismiss),
        )
        .child(
            div()
                .absolute()
                .top(px(top))
                .left(px(left))
                .w(px(CONTEXT_W))
                .occlude()
                .py(px(4.0))
                .bg(theme::surface())
                .border_1()
                .border_color(theme::rule())
                .shadow_2xl()
                .children(rows),
        )
        .into_any_element()
}

fn context_item(
    id: &'static str,
    icon_path: &'static str,
    label: &'static str,
) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .h(px(CONTEXT_ROW_H))
        .px(px(10.0))
        .flex()
        .items_center()
        .gap(px(9.0))
        .cursor_pointer()
        .hover(|style| style.bg(theme::accent_soft()))
        .debug_selector(move || id.to_string())
        .child(icon(icon_path, 15.0, theme::muted()))
        .child(
            ui_text(theme::TEXT_BODY, theme::ink())
                .flex_1()
                .flex_shrink_0()
                .child(label),
        )
}
