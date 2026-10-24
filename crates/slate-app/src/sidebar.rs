//! Left rail: library overview, filters, tags, batch rename and activity.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, prelude::*, px, rgba};

use slate_core::batch;
use slate_core::model::Filter;

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, label, metric, micro, mini_button, section_header, text_button};

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
            .px(px(14.0))
            .pt(px(14.0))
            .pb(px(12.0))
            .flex()
            .flex_col()
            .gap(px(3.0))
            .child(
                div()
                    .text_size(px(15.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .truncate()
                    .child(self.project.name.clone()),
            )
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(9.0))
                    .text_color(theme::faint())
                    .truncate()
                    .child(
                        self.project_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "No project file".to_string()),
                    ),
            )
            .child(
                div()
                    .mt(px(12.0))
                    .flex()
                    .items_start()
                    .gap(px(20.0))
                    .child(metric("Photos", format!("{photo_count}")))
                    .child(metric("Selected", format!("{selected_count}")))
                    .child(metric("Tags", format!("{tag_count_total}"))),
            )
            .child(
                div()
                    .mt(px(9.0))
                    .flex()
                    .items_center()
                    .gap(px(6.0))
                    .child(div().w(px(5.0)).h(px(5.0)).bg(if fresh || self.dirty {
                        theme::faint()
                    } else {
                        theme::ink()
                    }))
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.0))
                            .text_color(theme::faint())
                            .child(if fresh {
                                "Empty"
                            } else if self.dirty {
                                "Unsaved changes"
                            } else {
                                "Saved"
                            }),
                    ),
            );

        let import_row = div()
            .px(px(14.0))
            .pb(px(14.0))
            .flex()
            .gap(px(6.0))
            .child(
                button("sidebar-import-files", "Import files")
                    .flex_1()
                    .on_click(cx.listener(|this, _, _, cx| this.import_prompt(false, cx))),
            )
            .child(
                button("sidebar-import-folder", "Folder")
                    .flex_1()
                    .on_click(cx.listener(|this, _, _, cx| this.import_prompt(true, cx))),
            );

        let filter_block = div()
            .px(px(14.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .gap(px(8.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("Filter"))
                    .child(micro(format!("{} shown", self.visible_count()))),
            )
            .child(self.search_field.clone())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.0))
                            .text_color(theme::faint())
                            .child(format!("{selected_count} selected")),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(1.0))
                            .child(text_button("sidebar-all", "All").on_click(
                                cx.listener(|this, _, _, cx| this.select_all_visible(cx)),
                            ))
                            .child(
                                text_button("sidebar-none", "None")
                                    .on_click(cx.listener(|this, _, _, cx| this.deselect_all(cx))),
                            )
                            .child(
                                text_button("sidebar-invert", "Invert").on_click(
                                    cx.listener(|this, _, _, cx| this.invert_visible(cx)),
                                ),
                            ),
                    ),
            )
            .child(
                text_button("remove-selected", "Remove from project")
                    .on_click(cx.listener(|this, _, _, cx| this.remove_selected(cx))),
            );

        let mut filter_rows: Vec<AnyElement> = Vec::new();
        let is_all = matches!(self.filter, Filter::All);
        filter_rows.push(
            filter_row("filter-all", "All images", photo_count, is_all)
                .on_click(cx.listener(|this, _, _, cx| {
                    this.filter = Filter::All;
                    this.refresh_preview(cx);
                    cx.notify();
                }))
                .into_any_element(),
        );
        let is_untagged = matches!(self.filter, Filter::Untagged);
        filter_rows.push(
            filter_row("filter-untagged", "Untagged", untagged, is_untagged)
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
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(14.0))
                    .pt(px(12.0))
                    .pb(px(6.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("Tags"))
                    .child(micro(format!("{tag_count_total}"))),
            )
            .children(filter_rows)
            .child(
                div()
                    .px(px(14.0))
                    .py(px(10.0))
                    .flex()
                    .flex_col()
                    .gap(px(6.0))
                    .child(self.tag_field.clone())
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(2.0))
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

        let target_ids = self.target_ids();
        let target_count = target_ids.len();
        let sample = target_ids
            .first()
            .and_then(|id| self.project.photo(*id))
            .map(|photo| {
                batch::preview_name(photo, self.rename_options.start, &self.rename_options)
            })
            .unwrap_or_else(|| "No photo in scope".to_string());

        let start = self.rename_options.start;
        let padding = self.rename_options.padding;
        let rename_block = div()
            .px(px(14.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("Batch rename"))
                    .child(micro(format!("{target_count} in scope"))),
            )
            .child(label("Prefix"))
            .child(self.prefix_field.clone())
            .child(label("Suffix"))
            .child(self.suffix_field.clone())
            .child(
                div()
                    .mt(px(2.0))
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(theme::faint())
                                    .child("Start"),
                            )
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
                                    .text_size(px(9.5))
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
                            .gap(px(4.0))
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(theme::faint())
                                    .child("Pad"),
                            )
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
                                    .text_size(px(9.5))
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
                    .mt(px(2.0))
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.0))
                            .text_color(theme::faint())
                            .child("Next"),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(9.0))
                            .text_color(theme::muted())
                            .truncate()
                            .child(sample),
                    ),
            )
            .child(
                button("rename-preview", "Preview rename")
                    .w_full()
                    .mt(px(2.0))
                    .on_click(cx.listener(|this, _, _, cx| this.preview_rename(cx))),
            );

        let log_block = div()
            .px(px(14.0))
            .py(px(12.0))
            .border_t_1()
            .border_color(theme::rule())
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(label("Activity"))
            .children(self.log.iter().rev().take(4).map(|line| {
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(8.5))
                    .text_color(theme::faint())
                    .truncate()
                    .child(line.clone())
                    .into_any_element()
            }));

        div()
            .w(px(256.0))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_r_1()
            .border_color(theme::rule())
            .child(section_header("01", "Library", format!("{photo_count:04}")))
            .child(
                div()
                    .id("sidebar-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(project_block)
                    .child(import_row)
                    .child(filter_block)
                    .child(tags_block)
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
        .h(px(28.0))
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .text_size(px(10.0))
        .text_color(if active { theme::ink() } else { theme::muted() })
        .bg(if active {
            theme::accent_soft()
        } else {
            rgba(0x00000000)
        });
    let base = if active {
        base
    } else {
        base.hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
    };
    base.child(
        div()
            .flex()
            .items_center()
            .gap(px(7.0))
            .child(div().w(px(4.0)).h(px(4.0)).bg(if active {
                theme::ink()
            } else {
                rgba(0x00000000)
            }))
            .child(name.to_string()),
    )
    .child(
        div()
            .font_family(theme::FONT_MONO)
            .text_size(px(9.0))
            .text_color(if active {
                theme::muted()
            } else {
                theme::faint()
            })
            .child(format!("{count}")),
    )
}
