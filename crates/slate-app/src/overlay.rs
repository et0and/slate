//! Modal confirmation for destructive batch operations.

use gpui::{AnyElement, Context, div, prelude::*, px, rgba};

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, label, primary_button};

impl ContactSheet {
    pub(crate) fn render_overlay(&mut self, cx: &mut Context<Self>) -> Option<AnyElement> {
        self.rename_pending.as_ref()?;

        let plan = self.rename_pending.as_ref()?;
        let entries = plan.entries.len();
        let conflicts = plan.conflicts().count();
        let can_apply = plan.can_apply();

        let mut rows: Vec<AnyElement> = Vec::with_capacity(entries);
        for (index, entry) in plan.entries.iter().enumerate() {
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
            rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .px(px(18.0))
                    .py(px(9.0))
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(10.0))
                            .child(
                                div()
                                    .w(px(26.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(8.5))
                                    .text_color(theme::faint())
                                    .child(format!("{:03}", index + 1)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.5))
                                    .text_color(theme::muted())
                                    .truncate()
                                    .child(from),
                            )
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(11.0))
                                    .text_color(theme::faint())
                                    .child("→"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.5))
                                    .font_weight(if entry.conflict.is_some() {
                                        gpui::FontWeight::BOLD
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_color(theme::ink())
                                    .truncate()
                                    .child(to),
                            ),
                    )
                    .children(entry.conflict.as_ref().map(|conflict| {
                        div()
                            .pl(px(36.0))
                            .font_family(theme::FONT_UI)
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .text_size(px(9.0))
                            .text_color(theme::muted())
                            .child(format!("! {conflict}"))
                    }))
                    .into_any_element(),
            );
        }

        let apply: AnyElement = if can_apply {
            primary_button("rename-apply", "Apply rename")
                .on_click(cx.listener(|this, _, _, cx| this.apply_rename(cx)))
                .into_any_element()
        } else {
            div()
                .h(px(28.0))
                .px(px(12.0))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(theme::rule())
                .font_family(theme::FONT_UI)
                .text_size(px(10.5))
                .text_color(theme::faint())
                .child("Apply rename")
                .into_any_element()
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
            .shadow_lg()
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(18.0))
                    .py(px(16.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .text_size(px(15.0))
                                    .child("Batch rename"),
                            )
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(9.5))
                                    .text_color(theme::faint())
                                    .child(format!("{entries} files / {conflicts} conflict(s)")),
                            ),
                    )
                    .child(label("Review every change")),
            )
            .child(
                div()
                    .id("rename-list")
                    .flex()
                    .flex_col()
                    .children(rows)
                    .max_h(px(410.0))
                    .overflow_y_scroll(),
            )
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(18.0))
                    .py(px(14.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.0))
                            .text_color(theme::faint())
                            .child("Files are renamed on disk. This cannot be undone."),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
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
}
