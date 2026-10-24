//! Left rail: import, search, tags, rename and activity.

use gpui::{
    AnyElement, Context, ElementId, MouseButton, SharedString, Window, div, prelude::*, px, rgba,
};

use slate_core::batch;
use slate_core::model::Filter;

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, group_label, label, micro, mono_text, stepper, text_button, ui_text};

impl ContactSheet {
    pub(crate) fn render_sidebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let photo_count = self.project.photos.len();
        let selected_count = self.selected.len();
        let tag_counts = self.tag_counts();
        let tag_count_total = tag_counts.len();
        let untagged = self.untagged_count();

        let search_block = div()
            .px(px(16.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(label("Search"))
            .child(self.search_field.clone())
            .child(
                mono_text(theme::TEXT_META, theme::muted())
                    .child(format!("{selected_count} selected")),
            );

        let mut filter_rows: Vec<AnyElement> = Vec::new();
        let is_all = matches!(self.filter, Filter::All);
        filter_rows.push(
            filter_row("filter-all", "All images", photo_count, is_all)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::All;
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
        );
        let is_untagged = matches!(self.filter, Filter::Untagged);
        filter_rows.push(
            filter_row("filter-untagged", "Untagged", untagged, is_untagged)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::Untagged;
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
        );
        for (tag, count) in tag_counts.iter() {
            let active = matches!(&self.filter, Filter::Tag(active) if active == tag);
            let click_tag = tag.clone();
            filter_rows.push(
                filter_row(
                    ElementId::Name(SharedString::from(format!("filter-tag-{tag}"))),
                    &format!("#{tag}"),
                    *count,
                    active,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.filter = Filter::Tag(click_tag.clone());
                    this.filter_changed();
                    this.refresh_preview();
                    cx.notify();
                }))
                .into_any_element(),
            );
        }

        let tags_block = div()
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(16.0))
                    .pt(px(14.0))
                    .pb(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(group_label("Tags"))
                    .child(micro(format!("{tag_count_total}"))),
            )
            .children(filter_rows)
            .child(
                div()
                    .px(px(16.0))
                    .py(px(12.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(self.tag_field.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                button("tag-add", "Add tag").flex_1().on_click(
                                    cx.listener(|this, _, _, cx| this.apply_tag(cx, true)),
                                ),
                            )
                            .child(
                                text_button("tag-remove", "Remove")
                                    .on_click(cx.listener(|this, _, _, cx| this.remove_tag(cx))),
                            )
                            .child(
                                text_button("tag-clear", "Clear")
                                    .on_click(cx.listener(|this, _, _, cx| this.clear_tags(cx))),
                            ),
                    ),
            );

        let target_ids = self.scope_ids();
        let target_count = target_ids.len();
        let sample = target_ids
            .first()
            .and_then(|id| self.project.photo(*id))
            .map(|photo| {
                batch::preview_name(photo, self.rename_options.start, &self.rename_options)
            })
            .unwrap_or_else(|| "—".to_string());

        let start = self.rename_options.start;
        let padding = self.rename_options.padding;

        let rename_block = div()
            .px(px(16.0))
            .py(px(14.0))
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(group_label("Rename"))
                    .child(micro(format!("{target_count} in scope"))),
            )
            .child(label("Prefix"))
            .child(self.prefix_field.clone())
            .child(label("Suffix"))
            .child(self.suffix_field.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("Start"))
                    .child(stepper(
                        "start-minus",
                        "start-plus",
                        format!("{start:05}"),
                        cx.listener(|this, _, _, cx| {
                            this.rename_options.start = this.rename_options.start.saturating_sub(1);
                            cx.notify();
                        }),
                        cx.listener(|this, _, _, cx| {
                            this.rename_options.start = this.rename_options.start.saturating_add(1);
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
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
                            this.rename_options.padding = (this.rename_options.padding + 1).min(8);
                            cx.notify();
                        }),
                    )),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(3.0))
                    .child(label("Example"))
                    .child(
                        mono_text(theme::TEXT_META, theme::muted())
                            .truncate()
                            .child(sample),
                    ),
            )
            .child(
                button("rename-review", "Review")
                    .w_full()
                    .on_click(cx.listener(|this, _, _, cx| this.preview_rename(cx))),
            );

        div()
            .w(px(self.panels.left()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(
                div()
                    .h(px(theme::WINDOW_CHROME_TOP))
                    .w_full()
                    .flex_shrink_0()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|_, _, window, _| window.start_window_move()),
                    ),
            )
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .px(px(16.0))
                            .pt(px(14.0))
                            .pb(px(10.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                ui_text(theme::TEXT_TITLE, theme::ink())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(self.project.name.clone()),
                            )
                            .child(
                                mono_text(theme::TEXT_META, theme::faint())
                                    .truncate()
                                    .child(
                                        self.project_path
                                            .as_ref()
                                            .map(|path| compact_path(path))
                                            .unwrap_or_else(|| "No file".to_string()),
                                    ),
                            ),
                    )
                    .child(search_block)
                    .child(tags_block)
                    .child(rename_block),
            )
            .into_any_element()
    }
}

fn filter_row(
    id: impl Into<ElementId>,
    name: &str,
    count: usize,
    active: bool,
) -> gpui::Stateful<gpui::Div> {
    let base = div()
        .id(id)
        .h(px(theme::ROW_H))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .cursor_pointer()
        .border_b_1()
        .border_color(theme::rule())
        .bg(if active {
            theme::accent_soft()
        } else {
            rgba(0x00000000)
        })
        .active(|style| style.opacity(0.8));
    let base = if active {
        base
    } else {
        base.hover(|style| style.bg(theme::accent_soft()))
    };
    base.child(
        div()
            .flex()
            .items_center()
            .gap(px(8.0))
            .child(div().w(px(3.0)).h(px(3.0)).bg(if active {
                theme::accent()
            } else {
                rgba(0x00000000)
            }))
            .child(
                ui_text(
                    theme::TEXT_BODY,
                    if active { theme::ink() } else { theme::muted() },
                )
                .child(name.to_string()),
            ),
    )
    .child(
        mono_text(
            theme::TEXT_META,
            if active {
                theme::muted()
            } else {
                theme::faint()
            },
        )
        .child(format!("{count}")),
    )
}

fn compact_path(path: &std::path::Path) -> String {
    let home = std::env::var_os("HOME").or_else(|| std::env::var_os("USERPROFILE"));
    let Some(home) = home else {
        return path.display().to_string();
    };
    match path.strip_prefix(std::path::PathBuf::from(home)) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}
