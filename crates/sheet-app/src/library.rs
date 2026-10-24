//! The image library grid.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px, rgba};

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, micro, primary_button, rule};

impl ContactSheet {
    pub(crate) fn render_library(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visible = self.visible_indices();
        let selected_visible = visible
            .iter()
            .filter(|index| self.selected.contains(&self.project.photos[**index].id))
            .count();

        let toolbar = div()
            .h(px(34.0))
            .flex_shrink_0()
            .px(px(12.0))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(theme::ink())
            .bg(theme::paper())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(10.0))
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(9.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("02 / IMAGES"),
                    )
                    .child(micro(self.filter_label()))
                    .child(micro(if self.search.is_empty() {
                        String::new()
                    } else {
                        format!("/ \"{}\"", self.search.to_uppercase())
                    })),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(micro(format!(
                        "SHOWING {:04} / {:04}",
                        visible.len(),
                        self.project.photos.len()
                    )))
                    .child(micro(format!("SELECTED {:04}", selected_visible))),
            );

        let body: AnyElement = if self.project.photos.is_empty() {
            self.render_empty_state(cx)
        } else if visible.is_empty() {
            self.render_no_matches(cx)
        } else {
            let mut cells: Vec<AnyElement> = Vec::with_capacity(visible.len());
            for index in visible {
                let photo = &self.project.photos[index];
                let id = photo.id;
                let selected = self.selected.contains(&id);
                let name = photo.file_name();
                let dimensions = if photo.width == 0 || photo.height == 0 {
                    "NO PREVIEW".to_string()
                } else {
                    format!("{}×{}", photo.width, photo.height)
                };
                let tags = if photo.tags.is_empty() {
                    "NO TAGS".to_string()
                } else {
                    photo
                        .tags
                        .iter()
                        .take(2)
                        .map(|tag| format!("#{tag}"))
                        .collect::<Vec<_>>()
                        .join(" ")
                };
                let bytes = format_bytes(photo.bytes);
                let index_label = format!("{:03}", index + 1);
                let image = self.thumb_image(id);
                let failure = self.thumb_failed(id).map(str::to_string);

                let preview: AnyElement = if let Some(image) = image {
                    img(image)
                        .object_fit(gpui::ObjectFit::Contain)
                        .size_full()
                        .into_any_element()
                } else if failure.is_some() {
                    div()
                        .size_full()
                        .flex()
                        .flex_col()
                        .items_center()
                        .justify_center()
                        .gap(px(3.0))
                        .child(
                            div()
                                .font_family(theme::FONT_MONO)
                                .text_size(px(8.0))
                                .text_color(theme::danger())
                                .child("NO PREVIEW"),
                        )
                        .child(
                            div()
                                .font_family(theme::FONT_MONO)
                                .text_size(px(8.0))
                                .text_color(theme::faint())
                                .child("UNSUPPORTED OR MISSING"),
                        )
                        .into_any_element()
                } else {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family(theme::FONT_MONO)
                        .text_size(px(8.0))
                        .text_color(theme::faint())
                        .child("LOADING")
                        .into_any_element()
                };

                let inner = div()
                    .w(px(174.0))
                    .flex()
                    .flex_col()
                    .bg(theme::white())
                    .border_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .w_full()
                            .h(px(132.0))
                            .bg(theme::photo_bg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .overflow_hidden()
                            .child(preview),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(theme::rule())
                            .px(px(6.0))
                            .py(px(4.0))
                            .flex()
                            .flex_col()
                            .gap(px(2.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .child(
                                        div()
                                            .font_family(theme::FONT_MONO)
                                            .text_size(px(8.0))
                                            .text_color(if selected {
                                                theme::accent()
                                            } else {
                                                theme::faint()
                                            })
                                            .child(if selected {
                                                format!("■ {index_label}")
                                            } else {
                                                index_label.clone()
                                            }),
                                    )
                                    .child(
                                        div()
                                            .font_family(theme::FONT_MONO)
                                            .text_size(px(8.0))
                                            .text_color(theme::faint())
                                            .child(dimensions),
                                    ),
                            )
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(9.0))
                                    .text_color(theme::ink())
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(4.0))
                                    .child(
                                        div()
                                            .font_family(theme::FONT_MONO)
                                            .text_size(px(8.0))
                                            .text_color(if photo.tags.is_empty() {
                                                theme::faint()
                                            } else {
                                                theme::muted()
                                            })
                                            .flex_1()
                                            .min_w(px(0.0))
                                            .truncate()
                                            .child(tags),
                                    )
                                    .child(
                                        div()
                                            .font_family(theme::FONT_MONO)
                                            .text_size(px(8.0))
                                            .text_color(theme::faint())
                                            .child(bytes),
                                    ),
                            ),
                    );

                let hover_border = if selected {
                    theme::ink()
                } else {
                    theme::faint()
                };

                cells.push(
                    div()
                        .id(ElementId::Name(SharedString::from(format!(
                            "cell-{}",
                            id.0
                        ))))
                        .p(px(2.0))
                        .border_1()
                        .border_color(if selected {
                            theme::ink()
                        } else {
                            rgba(0x00000000)
                        })
                        .cursor_pointer()
                        .hover(move |style| style.border_color(hover_border))
                        .on_click(cx.listener(move |this, _, _, cx| this.toggle_selected(id, cx)))
                        .child(inner)
                        .into_any_element(),
                );
            }

            div()
                .id("library-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p(px(14.0))
                .child(div().flex().flex_wrap().gap(px(10.0)).children(cells))
                .into_any_element()
        };

        div()
            .flex_1()
            .min_w(px(0.0))
            .min_h(px(0.0))
            .flex()
            .flex_col()
            .bg(theme::paper())
            .child(toolbar)
            .child(body)
            .into_any_element()
    }

    fn render_empty_state(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .child(
                div()
                    .w(px(460.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(30.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("NO PHOTOS."),
                    )
                    .child(rule())
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(9.0))
                            .text_color(theme::muted())
                            .child(
                                "IMPORT FILES OR A FOLDER / DRAG PHOTOS ANYWHERE. TAG AND RENAME IN BULK, THEN BUILD A CONTACT SHEET.",
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .gap(px(8.0))
                            .mt(px(6.0))
                            .child(
                                primary_button("empty-import-files", "IMPORT FILES")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_prompt(false, cx)
                                    })),
                            )
                            .child(
                                button("empty-import-folder", "IMPORT FOLDER")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_prompt(true, cx)
                                    })),
                            )
                            .child(
                                button("empty-open", "OPEN PROJECT").on_click(
                                    cx.listener(|this, _, _, cx| this.open_project_prompt(cx)),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(10.0))
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::faint())
                            .child(
                                "JPG / JPEG / PNG / TIFF / WEBP / GIF / BMP / HEIC / DNG / CR2 / CR3 / NEF / ARW / RAF / RW2 / ORF / PEF / SRW",
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_no_matches(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p(px(40.0))
            .child(
                div()
                    .w(px(400.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(26.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("NO MATCHES."),
                    )
                    .child(rule())
                    .child(micro(format!(
                        "FILTER {} / SEARCH \"{}\"",
                        self.filter_label(),
                        self.search.to_uppercase()
                    )))
                    .child(
                        button("clear-filters", "CLEAR FILTERS")
                            .on_click(cx.listener(|this, _, _, cx| this.clear_filters(cx))),
                    ),
            )
            .into_any_element()
    }
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "—".to_string()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}
