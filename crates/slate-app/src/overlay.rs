//! Modal confirmation for destructive batch operations.

use std::sync::Arc;

use gpui::{AnyElement, Context, Window, div, prelude::*, px, rgba, uniform_list};

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{
    button, disabled_button, icon, icon_button, mono_text, primary_button, ui_text,
};

impl ContactSheet {
    pub(crate) fn render_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        if self.about_open {
            return Some(self.render_about(cx));
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
                                ui_text(16.0, theme::ink())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .line_height(px(21.0))
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
                            .line_height(px(13.0))
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

    fn render_about(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let version = env!("CARGO_PKG_VERSION");
        let close = cx.listener(|this, _, _, cx| {
            this.about_open = false;
            cx.notify();
        });

        let panel = div()
            .w(px(440.0))
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
                        ui_text(14.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("About"),
                    )
                    .child(icon_button("about-close-icon", assets::X).on_click(close)),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(18.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        ui_text(18.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Slate Desktop"),
                    )
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .child(format!("Version {version}")),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY + 0.5, theme::muted())
                            .line_height(px(18.0))
                            .child("A monochrome contact-sheet builder for photographers."),
                    )
                    .child(div().h(px(1.0)).w_full().bg(theme::rule()))
                    .child(
                        ui_text(theme::TEXT_META + 0.5, theme::muted())
                            .line_height(px(16.0))
                            .child("Built with GPUI and Rust."),
                    )
                    .child(
                        ui_text(theme::TEXT_META + 0.5, theme::muted())
                            .line_height(px(16.0))
                            .child("Geist and Geist Mono, SIL Open Font License."),
                    )
                    .child(
                        ui_text(theme::TEXT_META + 0.5, theme::muted())
                            .line_height(px(16.0))
                            .child("Sample photographs from Unsplash."),
                    )
                    .child(
                        ui_text(theme::TEXT_META + 0.5, theme::muted())
                            .line_height(px(16.0))
                            .child("MIT License."),
                    ),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(14.0))
                    .flex()
                    .justify_end()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(button("about-close", "Close").on_click(cx.listener(
                        |this, _, _, cx| {
                            this.about_open = false;
                            cx.notify();
                        },
                    ))),
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
                        cx.listener(|this, _, _, cx| {
                            this.about_open = false;
                            cx.notify();
                        }),
                    ),
            )
            .child(panel)
            .into_any_element()
    }
}
