//! Modal confirmation for destructive batch operations.

use std::sync::Arc;

use gpui::{AnyElement, Context, Window, div, img, prelude::*, px, rgba, uniform_list};

use crate::app::{ContactSheet, PhotoMetaState};
use crate::assets;
use crate::theme;
use crate::widgets::{
    button, disabled_button, icon, icon_button, mono_text, primary_button, ui_text,
};

impl ContactSheet {
    pub(crate) fn render_overlay(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        if let Some(menu) = self.render_photo_context(window, cx) {
            return Some(menu);
        }
        if self.rename_photo_target.is_some() {
            return Some(self.render_photo_rename(cx));
        }
        if self.photo_info_target.is_some() {
            return Some(self.render_photo_info(cx));
        }
        if let Some(notice) = self.render_drive_notice(cx) {
            return Some(notice);
        }
        if let Some(picker) = self.render_drive_picker(cx) {
            return Some(picker);
        }
        let plan = self.rename_pending.clone()?;
        let entries = plan.entries.len();
        let conflicts = plan.conflicts;
        let can_apply = plan.can_apply();
        let file_label = if entries == 1 { "file" } else { "files" };
        let conflict_label = if conflicts == 1 {
            "conflict"
        } else {
            "conflicts"
        };

        const RENAME_ROW_H: f32 = 76.0;
        let list_height = (entries as f32 * RENAME_ROW_H).clamp(1.0, 410.0);
        let plan_for_list = Arc::clone(&plan);
        let list = uniform_list(
            "rename-list",
            entries,
            cx.processor(
                move |_: &mut ContactSheet,
                      range: std::ops::Range<usize>,
                      _: &mut Window,
                      _: &mut Context<ContactSheet>| {
                    range
                        .map(|index| {
                            let entry = &plan_for_list.entries[index];
                            let from = entry
                                .from
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            let to = entry
                                .to
                                .file_name()
                                .map(|name| name.to_string_lossy().into_owned())
                                .unwrap_or_default();
                            div()
                                .h(px(RENAME_ROW_H))
                                .flex()
                                .flex_col()
                                .justify_center()
                                .gap(px(4.0))
                                .px(px(20.0))
                                .py(px(10.0))
                                .border_b_1()
                                .border_color(theme::rule())
                                .child(
                                    div()
                                        .flex()
                                        .items_center()
                                        .gap(px(10.0))
                                        .child(
                                            mono_text(theme::TEXT_MICRO, theme::faint())
                                                .w(px(26.0))
                                                .child(format!("{:03}", index + 1)),
                                        )
                                        .child(
                                            mono_text(theme::TEXT_META, theme::muted())
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .truncate()
                                                .child(from),
                                        )
                                        .child(icon(assets::ARROW_RIGHT, 12.0, theme::faint()))
                                        .child(
                                            mono_text(theme::TEXT_META, theme::ink())
                                                .flex_1()
                                                .min_w(px(0.0))
                                                .font_weight(if entry.conflict.is_some() {
                                                    gpui::FontWeight::SEMIBOLD
                                                } else {
                                                    gpui::FontWeight::NORMAL
                                                })
                                                .truncate()
                                                .child(to),
                                        ),
                                )
                                .children(entry.conflict.as_ref().map(|conflict| {
                                    div()
                                        .pl(px(36.0))
                                        .flex()
                                        .items_center()
                                        .gap(px(6.0))
                                        .child(icon(assets::ALERT, 11.0, theme::muted()))
                                        .child(
                                            ui_text(theme::TEXT_META, theme::muted())
                                                .font_weight(gpui::FontWeight::MEDIUM)
                                                .child(conflict.clone()),
                                        )
                                }))
                                .into_any_element()
                        })
                        .collect::<Vec<_>>()
                },
            ),
        )
        .track_scroll(self.rename_scroll.clone())
        .h(px(list_height));

        let apply: AnyElement = if can_apply {
            primary_button("rename-apply", "Apply rename")
                .on_click(cx.listener(|this, _, _, cx| this.apply_rename(cx)))
                .into_any_element()
        } else {
            disabled_button("Apply rename").into_any_element()
        };

        let panel = div()
            .w(px(720.0))
            .max_h(px(600.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::rule())
            .shadow_2xl()
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(20.0))
                    .py(px(18.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                ui_text(20.0, theme::ink())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(23.0))
                                    .child("Review changes"),
                            )
                            .child(mono_text(theme::TEXT_META, theme::faint()).child(format!(
                                "{entries} {file_label} / {conflicts} {conflict_label}"
                            ))),
                    )
                    .child(
                        icon_button("rename-cancel-close", assets::X)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_rename(cx))),
                    ),
            )
            .child(list)
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(
                        ui_text(theme::TEXT_META, theme::faint())
                            .line_height(px(15.0))
                            .child("Files are renamed on disk. This cannot be undone."),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                button("rename-cancel", "Cancel")
                                    .on_click(cx.listener(|this, _, _, cx| this.cancel_rename(cx))),
                            )
                            .child(apply),
                    ),
            );

        Some(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000088))
                .child(panel)
                .into_any_element(),
        )
    }

    fn render_photo_context(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let menu = self.photo_context.clone()?;
        let viewport = window.viewport_size();
        let width = 224.0_f32;
        let height = 5.0 * 36.0 + 2.0 + 8.0;
        let left = f32::from(menu.position.x)
            .min(f32::from(viewport.width) - width - 8.0)
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
        let mut rows: Vec<AnyElement> = Vec::new();
        rows.push(
            div()
                .id("photo-context-rename")
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    this.begin_photo_rename(photo_id, window, cx)
                }))
                .child(icon(assets::PENCIL, 15.0, theme::muted()))
                .child(ui_text(theme::TEXT_BODY, theme::ink()).child("Rename…"))
                .into_any_element(),
        );
        rows.push(
            div()
                .id("photo-context-info")
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .on_click(cx.listener(move |this, _, _, cx| this.open_photo_info(photo_id, cx)))
                .child(icon(assets::INFO, 15.0, theme::muted()))
                .child(ui_text(theme::TEXT_BODY, theme::ink()).child("Properties…"))
                .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            div()
                .id("photo-context-reveal")
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .on_click(cx.listener(move |this, _, _, cx| this.reveal_photo(photo_id, cx)))
                .child(icon(assets::FOLDER, 15.0, theme::muted()))
                .child(ui_text(theme::TEXT_BODY, theme::ink()).child(reveal_label))
                .into_any_element(),
        );
        rows.push(
            div()
                .id("photo-context-copy")
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .on_click(cx.listener(move |this, _, _, cx| this.copy_photo_path(photo_id, cx)))
                .child(icon(assets::COPY, 15.0, theme::muted()))
                .child(ui_text(theme::TEXT_BODY, theme::ink()).child("Copy file path"))
                .into_any_element(),
        );
        rows.push(separator());
        rows.push(
            div()
                .id("photo-context-remove")
                .h(px(36.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .gap(px(9.0))
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .on_click(cx.listener(move |this, _, _, cx| this.remove_photo(photo_id, cx)))
                .child(icon(assets::TRASH, 15.0, theme::muted()))
                .child(ui_text(theme::TEXT_BODY, theme::ink()).child("Remove from project"))
                .into_any_element(),
        );

        Some(
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
                        .on_mouse_down(
                            gpui::MouseButton::Left,
                            cx.listener(|this, _, _, cx| {
                                this.photo_context = None;
                                cx.notify();
                            }),
                        ),
                )
                .child(
                    div()
                        .absolute()
                        .top(px(top))
                        .left(px(left))
                        .w(px(width))
                        .py(px(4.0))
                        .bg(theme::surface())
                        .border_1()
                        .border_color(theme::rule())
                        .shadow_2xl()
                        .children(rows),
                )
                .into_any_element(),
        )
    }

    fn render_photo_info(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let Some(photo_id) = self.photo_info_target else {
            return div().into_any_element();
        };
        let Some(photo) = self.project.photo(photo_id).cloned() else {
            self.photo_info_target = None;
            return div().into_any_element();
        };
        let image = self.preview_image(photo.id(), photo.rotation(), false);
        let dimensions = photo
            .oriented_size()
            .map(|(width, height)| format!("{width} × {height}"))
            .unwrap_or_else(|| "—".to_string());
        let tags = if photo.tags.is_empty() {
            "None".to_string()
        } else {
            photo
                .tags
                .iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join("  ")
        };
        let folder = photo
            .path
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_else(|| "—".to_string());
        let metadata = match self.photo_meta.get(&photo_id) {
            Some(PhotoMetaState::Loading) => None,
            Some(PhotoMetaState::Ready(meta)) => Some(meta),
            None => None,
        };
        let error = self.photo_date_error.clone();

        let thumbnail: AnyElement = match image {
            Some(image) => img(image)
                .object_fit(gpui::ObjectFit::Contain)
                .size_full()
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(icon(assets::IMAGE, 28.0, theme::faint()))
                .into_any_element(),
        };

        let mut exif_rows: Vec<AnyElement> = Vec::new();
        match metadata {
            Some(meta) if meta.rows.is_empty() => {
                exif_rows.push(
                    ui_text(theme::TEXT_META, theme::faint())
                        .child("No EXIF metadata found.")
                        .into_any_element(),
                );
            }
            Some(meta) => {
                for (label, value) in &meta.rows {
                    exif_rows.push(info_row(label, value));
                }
            }
            None => {
                exif_rows.push(
                    ui_text(theme::TEXT_META, theme::faint())
                        .child("Reading EXIF…")
                        .into_any_element(),
                );
            }
        }

        let panel = div()
            .w(px(580.0))
            .max_h(px(660.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::rule())
            .shadow_2xl()
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(22.0))
                    .py(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        ui_text(theme::TEXT_TITLE, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Photo properties"),
                    )
                    .child(
                        icon_button("photo-info-close", assets::X)
                            .on_click(cx.listener(|this, _, _, cx| this.close_photo_info(cx))),
                    ),
            )
            .child(
                div()
                    .id("photo-info-body")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .px(px(22.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(18.0))
                    .child(
                        div()
                            .flex()
                            .items_start()
                            .gap(px(16.0))
                            .child(
                                div()
                                    .w(px(184.0))
                                    .h(px(132.0))
                                    .flex_shrink_0()
                                    .overflow_hidden()
                                    .bg(theme::photo_bg())
                                    .border_1()
                                    .border_color(theme::rule())
                                    .child(thumbnail),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .flex()
                                    .flex_col()
                                    .gap(px(5.0))
                                    .child(
                                        ui_text(theme::TEXT_BODY, theme::ink())
                                            .font_weight(gpui::FontWeight::MEDIUM)
                                            .truncate()
                                            .child(photo.file_name()),
                                    )
                                    .child(
                                        mono_text(theme::TEXT_META, theme::faint())
                                            .child(dimensions),
                                    )
                                    .child(
                                        mono_text(theme::TEXT_META, theme::faint())
                                            .child(format_bytes(photo.bytes)),
                                    )
                                    .child(
                                        mono_text(theme::TEXT_META, theme::faint())
                                            .truncate()
                                            .child(folder),
                                    )
                                    .child(
                                        ui_text(theme::TEXT_META, theme::muted())
                                            .truncate()
                                            .child(tags),
                                    ),
                            ),
                    )
                    .child(section_label("EXIF"))
                    .child(div().flex().flex_col().children(exif_rows))
                    .child(section_label("Capture date"))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(7.0))
                            .child(self.photo_date_field.clone())
                            .children(error.map(|error| {
                                ui_text(theme::TEXT_META, theme::muted())
                                    .line_height(px(16.0))
                                    .child(error)
                            }))
                            .child(
                                ui_text(theme::TEXT_MICRO, theme::faint())
                                    .line_height(px(14.0))
                                    .child(
                                        "Stored in the .slate project and used for date-based names. Original EXIF is not modified.",
                                    ),
                            )
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        button("photo-info-file-date", "Use file date")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.use_file_date(cx)
                                            })),
                                    )
                                    .child(
                                        primary_button("photo-info-save-date", "Save date")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.save_photo_date(cx)
                                            })),
                                    ),
                            ),
                    ),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(22.0))
                    .py(px(14.0))
                    .flex()
                    .justify_end()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(
                        button("photo-info-done", "Close")
                            .on_click(cx.listener(|this, _, _, cx| this.close_photo_info(cx))),
                    ),
            );

        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .bg(rgba(0x00000088))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.close_photo_info(cx)),
                    ),
            )
            .child(panel)
            .into_any_element()
    }

    fn render_photo_rename(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let current = self
            .rename_photo_target
            .and_then(|id| self.project.photo(id))
            .map(|photo| photo.file_name())
            .unwrap_or_default();
        let error = self.rename_photo_error.clone();
        let panel = div()
            .w(px(460.0))
            .flex()
            .flex_col()
            .overflow_hidden()
            .bg(theme::surface())
            .border_1()
            .border_color(theme::rule())
            .shadow_2xl()
            .child(
                div()
                    .px(px(20.0))
                    .py(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        ui_text(theme::TEXT_TITLE, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Rename photo"),
                    )
                    .child(
                        icon_button("photo-rename-close", assets::X)
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_photo_rename(cx))),
                    ),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .truncate()
                            .child(current),
                    )
                    .child(self.rename_photo_field.clone())
                    .children(error.map(|error| {
                        ui_text(theme::TEXT_META, theme::muted())
                            .line_height(px(16.0))
                            .child(error)
                    })),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(14.0))
                    .flex()
                    .justify_end()
                    .gap(px(8.0))
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(
                        button("photo-rename-cancel", "Cancel")
                            .on_click(cx.listener(|this, _, _, cx| this.cancel_photo_rename(cx))),
                    )
                    .child(
                        primary_button("photo-rename-apply", "Rename")
                            .on_click(cx.listener(|this, _, _, cx| this.apply_photo_rename(cx))),
                    ),
            );
        div()
            .absolute()
            .top(px(0.0))
            .left(px(0.0))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .absolute()
                    .top(px(0.0))
                    .left(px(0.0))
                    .size_full()
                    .bg(rgba(0x00000088))
                    .on_mouse_down(
                        gpui::MouseButton::Left,
                        cx.listener(|this, _, _, cx| this.cancel_photo_rename(cx)),
                    ),
            )
            .child(panel)
            .into_any_element()
    }
}

fn section_label(label: &str) -> AnyElement {
    ui_text(theme::TEXT_LABEL, theme::muted())
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .child(label.to_string())
        .into_any_element()
}

fn info_row(label: &str, value: &str) -> AnyElement {
    div()
        .min_h(px(30.0))
        .flex()
        .items_center()
        .gap(px(12.0))
        .border_b_1()
        .border_color(theme::rule())
        .child(
            ui_text(theme::TEXT_LABEL, theme::faint())
                .w(px(112.0))
                .flex_shrink_0()
                .child(label.to_string()),
        )
        .child(
            ui_text(theme::TEXT_BODY, theme::ink())
                .min_w(px(0.0))
                .truncate()
                .child(value.to_string()),
        )
        .into_any_element()
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "Unknown size".to_string()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}
