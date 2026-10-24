//! Left rail: library overview, filters, tags, batch rename and the log.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, prelude::*, px, rgba};

use slate_core::batch;
use slate_core::model::Filter;

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{
    button, key_value, label, micro, mini_button, rule, section_header, text_button,
};

impl ContactSheet {
    pub(crate) fn render_sidebar(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let photo_count = self.project.photos.len();
        let selected_count = self.selected.len();
        let tag_counts = self.project.tag_counts();
        let tag_count_total = tag_counts.len();
        let untagged = self.project.untagged_count();
        let fresh = !self.dirty && self.project_path.is_none();

        let project_block = div()
            .px(px(10.0))
            .py(px(9.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .truncate()
                    .child(self.project.name.to_uppercase()),
            )
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(8.0))
                    .text_color(theme::faint())
                    .truncate()
                    .child(
                        self.project_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "NO PROJECT FILE".to_string()),
                    ),
            )
            .child(
                div()
                    .mt(px(4.0))
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(key_value("PHOTOS", format!("{photo_count:04}")))
                    .child(key_value("SELECTED", format!("{selected_count:04}")))
                    .child(key_value("TAGS", format!("{:04}", tag_counts.len())))
                    .child(key_value(
                        "STATE",
                        if fresh {
                            "EMPTY"
                        } else if self.dirty {
                            "UNSAVED"
                        } else {
                            "SAVED"
                        },
                    )),
            );

        let import_row = div()
            .px(px(10.0))
            .py(px(8.0))
            .flex()
            .gap(px(4.0))
            .child(
                button("sidebar-import-files", "FILES")
                    .flex_1()
                    .on_click(cx.listener(|this, _, _, cx| this.import_prompt(false, cx))),
            )
            .child(
                button("sidebar-import-folder", "FOLDER")
                    .flex_1()
                    .on_click(cx.listener(|this, _, _, cx| this.import_prompt(true, cx))),
            );

        let filter_block = div()
            .px(px(10.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("FILTER"))
                    .child(micro(format!("{:04} SHOWN", self.visible_count()))),
            )
            .child(self.search_field.clone())
            .child(
                div()
                    .flex()
                    .gap(px(4.0))
                    .child(
                        button("sidebar-all", "ALL")
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| this.select_all_visible(cx))),
                    )
                    .child(
                        button("sidebar-none", "NONE")
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| this.deselect_all(cx))),
                    )
                    .child(
                        button("sidebar-invert", "INV")
                            .flex_1()
                            .on_click(cx.listener(|this, _, _, cx| this.invert_visible(cx))),
                    ),
            )
            .child(
                text_button("remove-selected", "REMOVE FROM PROJECT / KEEPS FILES")
                    .on_click(cx.listener(|this, _, _, cx| this.remove_selected(cx))),
            )
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(8.0))
                    .text_color(theme::faint())
                    .child("NO SELECTION / SHEETS USE ALL VISIBLE"),
            );

        let mut filter_rows: Vec<AnyElement> = Vec::new();
        let is_all = matches!(self.filter, Filter::All);
        filter_rows.push(
            filter_row("filter-all", "ALL IMAGES", photo_count, is_all)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::All;
                    this.refresh_preview(cx);
                    cx.notify();
                }))
                .into_any_element(),
        );
        let is_untagged = matches!(self.filter, Filter::Untagged);
        filter_rows.push(
            filter_row("filter-untagged", "UNTAGGED", untagged, is_untagged)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::Untagged;
                    this.refresh_preview(cx);
                    cx.notify();
                }))
                .into_any_element(),
        );
        for (tag, count) in tag_counts {
            let active = matches!(&self.filter, Filter::Tag(active) if active == &tag);
            let click_tag = tag.clone();
            filter_rows.push(
                filter_row(
                    ElementId::Name(SharedString::from(format!("filter-tag-{tag}"))),
                    &format!("#{tag}"),
                    count,
                    active,
                )
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.filter = Filter::Tag(click_tag.clone());
                    this.refresh_preview(cx);
                    cx.notify();
                }))
                .into_any_element(),
            );
        }

        let tags_block = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(10.0))
                    .py(px(6.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("FILTER BY TAG"))
                    .child(micro(format!("{tag_count_total:02} TAGS"))),
            )
            .children(filter_rows)
            .child(
                div()
                    .px(px(10.0))
                    .py(px(8.0))
                    .flex()
                    .flex_col()
                    .gap(px(4.0))
                    .child(self.tag_field.clone())
                    .child(
                        div()
                            .flex()
                            .gap(px(4.0))
                            .child(
                                button("tag-add", "ADD TAG").flex_1().on_click(
                                    cx.listener(|this, _, _, cx| this.apply_tag(cx, true)),
                                ),
                            )
                            .child(
                                button("tag-remove", "REMOVE")
                                    .flex_1()
                                    .on_click(cx.listener(|this, _, _, cx| this.remove_tag(cx))),
                            )
                            .child(
                                text_button("tag-clear", "CLEAR")
                                    .on_click(cx.listener(|this, _, _, cx| this.clear_tags(cx))),
                            ),
                    ),
            );

        let target_ids = self.target_ids();
        let target_count = target_ids.len();
        let sample = target_ids
            .first()
            .and_then(|id| self.project.photo(*id))
            .map(|photo| {
                batch::preview_name(photo, self.rename_options.start, &self.rename_options)
            })
            .unwrap_or_else(|| "NO PHOTO IN SCOPE".to_string());

        let start = self.rename_options.start;
        let padding = self.rename_options.padding;
        let rename_block = div()
            .px(px(10.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("BATCH RENAME"))
                    .child(micro(format!("{target_count:04} IN SCOPE"))),
            )
            .child(label("PREFIX"))
            .child(self.prefix_field.clone())
            .child(label("SUFFIX"))
            .child(self.suffix_field.clone())
            .child(
                div()
                    .mt(px(2.0))
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .child(label("START"))
                            .child(mini_button("start-minus", "−").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.rename_options.start =
                                        this.rename_options.start.saturating_sub(1);
                                    cx.notify();
                                },
                            )))
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.0))
                                    .child(format!("{start:05}")),
                            )
                            .child(mini_button("start-plus", "+").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.rename_options.start =
                                        this.rename_options.start.saturating_add(1);
                                    cx.notify();
                                },
                            ))),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(3.0))
                            .child(label("PAD"))
                            .child(mini_button("pad-minus", "−").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.rename_options.padding =
                                        this.rename_options.padding.saturating_sub(1).max(1);
                                    cx.notify();
                                },
                            )))
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.0))
                                    .child(format!("{padding}")),
                            )
                            .child(mini_button("pad-plus", "+").on_click(cx.listener(
                                |this, _, _, cx| {
                                    this.rename_options.padding =
                                        (this.rename_options.padding + 1).min(8);
                                    cx.notify();
                                },
                            ))),
                    ),
            )
            .child(
                div()
                    .mt(px(3.0))
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(label("NEXT NAME"))
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::ink())
                            .truncate()
                            .child(sample),
                    ),
            )
            .child(
                button("rename-preview", "PREVIEW RENAME")
                    .w_full()
                    .mt(px(2.0))
                    .on_click(cx.listener(|this, _, _, cx| this.preview_rename(cx))),
            );

        let log_block = div()
            .px(px(10.0))
            .py(px(8.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .border_t_1()
            .border_color(theme::rule())
            .child(label("LOG"))
            .children(self.log.iter().rev().take(7).map(|line| {
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(8.0))
                    .text_color(theme::faint())
                    .truncate()
                    .child(line.clone())
                    .into_any_element()
            }));

        div()
            .w(px(252.0))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::rule())
            .child(section_header("01", "LIBRARY", format!("{photo_count:04}")))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(project_block)
                    .child(rule())
                    .child(import_row)
                    .child(rule())
                    .child(filter_block)
                    .child(rule())
                    .child(tags_block)
                    .child(rule())
                    .child(rename_block)
                    .child(log_block),
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
        .h(px(20.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_between()
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .text_size(px(8.0));
    let base = if active {
        base.bg(theme::ink()).text_color(theme::paper())
    } else {
        base.bg(rgba(0x00000000))
            .text_color(theme::ink())
            .hover(|style| style.bg(theme::accent_soft()))
    };
    base.child(name.to_string()).child(format!("{count:04}"))
}
