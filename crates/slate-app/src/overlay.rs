//! Modal confirmation for destructive batch operations.

use gpui::{AnyElement, Context, div, prelude::*, px, rgba};

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, micro_light, primary_button};

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
                    .gap(px(2.0))
                    .px(px(12.0))
                    .py(px(5.0))
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .w(px(24.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(8.0))
                                    .text_color(theme::faint())
                                    .child(format!("{:03}", index + 1)),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.0))
                                    .text_color(theme::muted())
                                    .truncate()
                                    .child(from),
                            )
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.0))
                                    .text_color(theme::faint())
                                    .child("→"),
                            )
                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.0))
                                    .text_color(if entry.conflict.is_some() {
                                        theme::danger()
                                    } else {
                                        theme::ink()
                                    })
                                    .truncate()
                                    .child(to),
                            ),
                    )
                    .children(entry.conflict.as_ref().map(|conflict| {
                        div()
                            .pl(px(32.0))
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::danger())
                            .child(conflict.clone())
                    }))
                    .into_any_element(),
            );
        }

        let apply: AnyElement = if can_apply {
            primary_button("rename-apply", "APPLY RENAME")
                .on_click(cx.listener(|this, _, _, cx| this.apply_rename(cx)))
                .into_any_element()
        } else {
            div()
                .h(px(28.0))
                .px(px(10.0))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(theme::rule())
                .font_family(theme::FONT_MONO)
                .text_size(px(9.0))
                .text_color(theme::faint())
                .child("APPLY RENAME")
                .into_any_element()
        };

        let panel = div()
            .w(px(760.0))
            .max_h(px(600.0))
            .flex()
            .flex_col()
            .bg(theme::paper())
            .border_1()
            .border_color(theme::ink())
            .child(
                div()
                    .h(px(30.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .bg(theme::ink())
                    .text_color(theme::paper())
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(9.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child(format!("BATCH RENAME / {entries} FILES")),
                    )
                    .child(micro_light(format!("{conflicts} CONFLICT(S)"))),
            )
            .child(
                div()
                    .id("rename-list")
                    .flex()
                    .flex_col()
                    .children(rows)
                    .max_h(px(420.0))
                    .overflow_y_scroll(),
            )
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .px(px(12.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::ink())
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::faint())
                            .child("FILES ARE RENAMED ON DISK / NO UNDO"),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(5.0))
                            .child(
                                button("rename-cancel", "CANCEL")
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
                .bg(rgba(0x11111099))
                .child(panel)
                .into_any_element(),
        )
    }
}
