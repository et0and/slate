//! The image library grid.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px};

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

        let filter_active = !matches!(&self.filter, slate_core::model::Filter::All);
        let filter_label = self.filter_label();
        let search = self.search.clone();
        let has_query = filter_active || !search.is_empty();

        let toolbar = div()
            .h(px(44.0))
            .flex_shrink_0()
            .px(px(16.0))
            .flex()
            .items_center()
            .justify_between()
            .border_b_1()
            .border_color(theme::rule())
            .bg(theme::paper())
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(9.0))
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(9.0))
                            .text_color(theme::faint())
                            .child("02"),
                    )
                    .child(
                        div()
                            .text_size(px(11.5))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Library"),
                    )
                    .child(
                        div()
                            .h(px(22.0))
                            .px(px(8.0))
                            .flex()
                            .items_center()
                            .bg(theme::accent_soft())
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.0))
                            .text_color(theme::muted())
                            .child(filter_label),
                    )
                    .children(if has_query {
                        Some(
                            button("clear-filters", "Clear")
                                .h(px(22.0))
                                .px(px(8.0))
                                .on_click(cx.listener(|this, _, _, cx| this.clear_filters(cx))),
                        )
                    } else {
                        None
                    })
                    .children(if search.is_empty() {
                        None
                    } else {
                        Some(
                            div()
                                .font_family(theme::FONT_MONO)
                                .text_size(px(9.0))
                                .text_color(theme::faint())
                                .child(format!("“{search}”")),
                        )
                    }),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(micro(format!("{} images", visible.len())))
                    .child(micro(format!("{selected_visible} selected"))),
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
                    "No preview".to_string()
                } else {
                    format!("{} × {}", photo.width, photo.height)
                };
                let tags = if photo.tags.is_empty() {
                    "No tags".to_string()
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
                        .gap(px(4.0))
                        .child(
                            div()
                                .font_family(theme::FONT_UI)
                                .text_size(px(9.5))
                                .text_color(theme::muted())
                                .child("No preview"),
                        )
                        .child(
                            div()
                                .font_family(theme::FONT_MONO)
                                .text_size(px(8.5))
                                .text_color(theme::faint())
                                .child("Unsupported or missing"),
                        )
                        .into_any_element()
                } else {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family(theme::FONT_UI)
                        .text_size(px(9.5))
                        .text_color(theme::faint())
                        .child("Loading")
                        .into_any_element()
                };

                let badge = selected.then(|| {
                    div()
                        .absolute()
                        .top(px(8.0))
                        .right(px(8.0))
                        .w(px(18.0))
                        .h(px(18.0))
                        .bg(theme::ink())
                        .flex()
                        .items_center()
                        .justify_center()
                        .font_family(theme::FONT_UI)
                        .text_size(px(10.0))
                        .text_color(theme::paper())
                        .child("✓")
                });

                let cell = div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "cell-{}",
                        id.0
                    ))))
                    .w(px(188.0))
                    .p(px(4.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::accent_soft()))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_selected(id, cx)))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(136.0))
                            .overflow_hidden()
                            .bg(theme::photo_bg())
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(preview)
                            .children(badge),
                    )
                    .child(
                        div()
                            .px(px(2.0))
                            .pt(px(8.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .font_weight(if selected {
                                        gpui::FontWeight::MEDIUM
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .text_size(px(10.5))
                                    .text_color(theme::ink())
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(6.0))
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(8.5))
                                    .text_color(theme::faint())
                                    .child(
                                        div()
                                            .truncate()
                                            .child(format!("{index_label}  {dimensions}")),
                                    )
                                    .child(bytes),
                            )
                            .child(
                                div()
                                    .font_family(theme::FONT_UI)
                                    .text_size(px(9.0))
                                    .text_color(if photo.tags.is_empty() {
                                        theme::faint()
                                    } else {
                                        theme::muted()
                                    })
                                    .truncate()
                                    .child(tags),
                            ),
                    );

                cells.push(cell.into_any_element());
            }

            div()
                .id("library-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p(px(16.0))
                .child(div().flex().flex_wrap().gap(px(14.0)).children(cells))
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
            .p(px(48.0))
            .child(
                div()
                    .w(px(420.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .text_size(px(30.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("No photos yet."),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(11.0))
                            .line_height(px(17.0))
                            .text_color(theme::muted())
                            .child(
                                "Import a folder, choose files, or drop photos anywhere. Tag and rename in bulk, then lay out a contact sheet.",
                            ),
                    )
                    .child(rule())
                    .child(
                        div()
                            .flex()
                            .gap(px(6.0))
                            .mt(px(2.0))
                            .child(
                                primary_button("empty-import-files", "Import files")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_prompt(false, cx)
                                    })),
                            )
                            .child(
                                button("empty-import-folder", "Import folder")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        this.import_prompt(true, cx)
                                    })),
                            )
                            .child(
                                button("empty-open", "Open project").on_click(
                                    cx.listener(|this, _, _, cx| this.open_project_prompt(cx)),
                                ),
                            ),
                    )
                    .child(
                        div()
                            .mt(px(8.0))
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.5))
                            .text_color(theme::faint())
                            .child(
                                "JPEG · PNG · TIFF · WebP · GIF · BMP · HEIC · DNG · CR2 · CR3 · NEF · ARW · RAF · RW2 · ORF · PEF · SRW",
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
            .p(px(48.0))
            .child(
                div()
                    .w(px(380.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        div()
                            .text_size(px(26.0))
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("No matches."),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(10.5))
                            .text_color(theme::muted())
                            .child(format!(
                                "Filter {} / search “{}”",
                                self.filter_label(),
                                self.search
                            )),
                    )
                    .child(
                        button("clear-filters-empty", "Clear filters")
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
