//! Native batch-rename window.

use std::sync::Arc;

use gpui::{
    AnyElement, Context, FocusHandle, Focusable, Render, Window, div, prelude::*, px, uniform_list,
};

use slate_core::batch;

use crate::app::{ContactSheet, Dismiss};
use crate::assets;
use crate::theme;
use crate::widgets::{
    button, disabled_button, group_label, icon, label, mono_text, primary_button, stepper,
    text_button, ui_text,
};

const RENAME_ROW_H: f32 = 76.0;

pub struct RenameWindow {
    parent: gpui::Entity<ContactSheet>,
    focus: FocusHandle,
}

impl RenameWindow {
    pub fn new(parent: gpui::Entity<ContactSheet>, cx: &mut Context<Self>) -> Self {
        cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        Self {
            parent,
            focus: cx.focus_handle(),
        }
    }
}

impl Focusable for RenameWindow {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for RenameWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let content = self.parent.update(cx, ContactSheet::render_rename_window);
        div()
            .size_full()
            .track_focus(&self.focus)
            .on_action(cx.listener(|this, _: &Dismiss, _, cx| {
                this.parent.update(cx, ContactSheet::cancel_rename);
            }))
            .bg(theme::paper())
            .text_color(theme::ink())
            .font_family(theme::FONT_UI)
            .child(content)
    }
}

impl ContactSheet {
    pub(crate) fn render_rename_window(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let target_ids = self.scope_ids();
        let target_count = target_ids.len();
        let sample = target_ids
            .first()
            .and_then(|id| self.project.photo(*id))
            .map_or_else(
                || "—".to_string(),
                |photo| batch::preview_name(photo, self.rename_options.start, &self.rename_options),
            );
        let start = self.rename_options.start;
        let padding = self.rename_options.padding;
        let plan = self.rename_pending.clone();

        let options = div()
            .flex_shrink_0()
            .px(px(22.0))
            .py(px(18.0))
            .flex()
            .flex_col()
            .gap(px(12.0))
            .border_b_1()
            .border_color(theme::rule())
            .child(
                div()
                    .grid()
                    .grid_cols(2)
                    .gap(px(14.0))
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(label("Prefix"))
                            .child(self.prefix_field.clone()),
                    )
                    .child(
                        div()
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(label("Suffix"))
                            .child(self.suffix_field.clone()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(28.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(label("Start"))
                            .child(stepper(
                                "start-minus",
                                "start-plus",
                                format!("{start:05}"),
                                cx.listener(|this, _, _, cx| {
                                    this.rename_options.start =
                                        this.rename_options.start.saturating_sub(1);
                                    cx.notify();
                                }),
                                cx.listener(|this, _, _, cx| {
                                    this.rename_options.start =
                                        this.rename_options.start.saturating_add(1);
                                    cx.notify();
                                }),
                            )),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(label("Digits"))
                            .child(stepper(
                                "pad-minus",
                                "pad-plus",
                                format!("{padding}"),
                                cx.listener(|this, _, _, cx| {
                                    this.rename_options.padding =
                                        this.rename_options.padding.saturating_sub(1).max(1);
                                    cx.notify();
                                }),
                                cx.listener(|this, _, _, cx| {
                                    this.rename_options.padding =
                                        (this.rename_options.padding + 1).min(8);
                                    cx.notify();
                                }),
                            )),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(label("Date tokens"))
                    .child(text_button("rename-date-token", "{date}").on_click(
                        cx.listener(|this, _, _, cx| this.append_rename_token("{date}", cx)),
                    ))
                    .child(text_button("rename-time-token", "{time}").on_click(
                        cx.listener(|this, _, _, cx| this.append_rename_token("{time}", cx)),
                    ))
                    .child(text_button("rename-datetime-token", "{datetime}").on_click(
                        cx.listener(|this, _, _, cx| this.append_rename_token("{datetime}", cx)),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap(px(16.0))
                    .child(
                        mono_text(theme::TEXT_META, theme::muted())
                            .flex_1()
                            .min_w(px(0.0))
                            .truncate()
                            .child(format!("Example: {sample}")),
                    )
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .child(format!("{target_count} in scope")),
                    )
                    .child(
                        primary_button("rename-review", "Review")
                            .on_click(cx.listener(|this, _, _, cx| this.preview_rename(cx))),
                    ),
            );

        let plan_panel: AnyElement = plan.map_or_else(
            || div().flex_1().into_any_element(),
            |plan| {
                let entries = plan.entries.len();
                let conflicts = plan.conflicts;
                let can_apply = plan.can_apply();
                let file_label = if entries == 1 { "file" } else { "files" };
                let conflict_label = if conflicts == 1 {
                    "conflict"
                } else {
                    "conflicts"
                };
                let list_height = (entries as f32 * RENAME_ROW_H).clamp(1.0, 410.0);
                let plan_for_list = Arc::clone(&plan);
                let list = uniform_list(
                    "rename-list",
                    entries,
                    cx.processor(
                        move |_: &mut Self,
                              range: std::ops::Range<usize>,
                              _: &mut Window,
                              _: &mut Context<Self>| {
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
                                                    mono_text(theme::TEXT_META, theme::faint())
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
                                                .child(icon(
                                                    assets::ARROW_RIGHT,
                                                    12.0,
                                                    theme::faint(),
                                                ))
                                                .child(
                                                    mono_text(theme::TEXT_META, theme::ink())
                                                        .flex_1()
                                                        .min_w(px(0.0))
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
                                                    ui_text(theme::TEXT_META, theme::ink())
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

                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .px(px(20.0))
                            .py(px(10.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_b_1()
                            .border_color(theme::rule())
                            .child(group_label("Review changes"))
                            .child(mono_text(theme::TEXT_META, theme::faint()).child(format!(
                                "{entries} {file_label} / {conflicts} {conflict_label}"
                            ))),
                    )
                    .child(list)
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(20.0))
                            .py(px(14.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_t_1()
                            .border_color(theme::rule())
                            .child(
                                ui_text(theme::TEXT_META, theme::faint())
                                    .child("Files are renamed on disk. This cannot be undone."),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(8.0))
                                    .child(button("rename-cancel", "Cancel").on_click(
                                        cx.listener(|this, _, _, cx| this.cancel_rename(cx)),
                                    ))
                                    .child(apply),
                            ),
                    )
                    .into_any_element()
            },
        );

        div()
            .size_full()
            .flex()
            .flex_col()
            .child(options)
            .child(plan_panel)
            .into_any_element()
    }
}
