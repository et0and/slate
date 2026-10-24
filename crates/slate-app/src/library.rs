//! The image library grid.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px};

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{button_with_icon, icon, mono_text, rule, ui_text};

const CELL_W: f32 = 185.0;
const THUMB_H: f32 = 136.0;

impl ContactSheet {
    pub(crate) fn render_library(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visible = self.visible_indices();
        let body: AnyElement = if self.project.photos.is_empty() {
            self.render_empty_state(cx)
        } else if visible.is_empty() {
            self.render_no_matches(cx)
        } else {
            let mut cells: Vec<AnyElement> = Vec::with_capacity(visible.len());
            for index in visible {
                let (id, name, dimensions, tags, bytes, rotation) = {
                    let photo = &self.project.photos[index];
                    let (width, height) = photo.oriented_size();
                    (
                        photo.id,
                        photo.file_name(),
                        if width == 0 || height == 0 {
                            "—".to_string()
                        } else {
                            format!("{width} × {height}")
                        },
                        if photo.tags.is_empty() {
                            None
                        } else {
                            Some(
                                photo
                                    .tags
                                    .iter()
                                    .take(2)
                                    .map(|tag| format!("#{tag}"))
                                    .collect::<Vec<_>>()
                                    .join(" "),
                            )
                        },
                        format_bytes(photo.bytes),
                        photo.rotation(),
                    )
                };
                let selected = self.selected.contains(&id);
                let index_label = format!("{:03}", index + 1);
                let image = self.thumb_image(id, rotation);
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
                        .items_center()
                        .justify_center()
                        .child(icon(assets::IMAGE_OFF, 20.0, theme::faint()))
                        .into_any_element()
                } else {
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(assets::IMAGE, 20.0, theme::faint()))
                        .opacity(0.45)
                        .into_any_element()
                };

                let rotate = div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "rotate-{}",
                        id.0
                    ))))
                    .absolute()
                    .top(px(9.0))
                    .right(px(35.0))
                    .size(px(20.0))
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::rule())
                    .shadow_sm()
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::accent_soft()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.rotate_photo(id, cx);
                    }))
                    .child(icon(assets::ROTATE, 12.0, theme::muted()));

                let badge = selected.then(|| {
                    div()
                        .absolute()
                        .top(px(9.0))
                        .right(px(9.0))
                        .size(px(20.0))
                        .bg(theme::accent())
                        .shadow_sm()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(icon(assets::CHECK, 11.0, theme::accent_text()))
                });

                let cell = div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "cell-{}",
                        id.0
                    ))))
                    .w(px(CELL_W))
                    .p(px(5.0))
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::accent_soft()))
                    .active(|style| style.opacity(0.82))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_selected(id, cx)))
                    .child(
                        div()
                            .relative()
                            .w_full()
                            .h(px(THUMB_H))
                            .overflow_hidden()
                            .bg(theme::photo_bg())
                            .border_1()
                            .border_color(theme::rule())
                            .shadow_sm()
                            .flex()
                            .items_center()
                            .justify_center()
                            .child(preview)
                            .child(rotate)
                            .children(badge),
                    )
                    .child(
                        div()
                            .px(px(1.0))
                            .pt(px(9.0))
                            .flex()
                            .flex_col()
                            .gap(px(4.0))
                            .child(
                                ui_text(theme::TEXT_BODY, theme::ink())
                                    .font_weight(if selected {
                                        gpui::FontWeight::MEDIUM
                                    } else {
                                        gpui::FontWeight::NORMAL
                                    })
                                    .line_height(px(15.0))
                                    .truncate()
                                    .child(name),
                            )
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .justify_between()
                                    .gap(px(6.0))
                                    .child(
                                        mono_text(theme::TEXT_MICRO, theme::faint())
                                            .truncate()
                                            .child(format!("{index_label}  {dimensions}")),
                                    )
                                    .child(
                                        mono_text(theme::TEXT_MICRO, theme::faint()).child(bytes),
                                    ),
                            )
                            .children(tags.map(|tags| {
                                ui_text(theme::TEXT_MICRO, theme::muted())
                                    .line_height(px(12.0))
                                    .truncate()
                                    .child(tags)
                            })),
                    );

                cells.push(cell.into_any_element());
            }

            div()
                .id("library-scroll")
                .flex_1()
                .min_h(px(0.0))
                .overflow_y_scroll()
                .p(px(20.0))
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
            .child(body)
            .into_any_element()
    }

    fn render_empty_state(&mut self, _cx: &mut Context<Self>) -> AnyElement {
        div()
            .flex_1()
            .flex()
            .items_center()
            .justify_center()
            .p(px(48.0))
            .child(
                div()
                    .w(px(440.0))
                    .flex()
                    .flex_col()
                    .gap(px(16.0))
                    .child(
                        div()
                            .size(px(34.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(theme::rule())
                            .bg(theme::surface())
                            .shadow_sm()
                            .child(icon(assets::IMAGE, 17.0, theme::muted())),
                    )
                    .child(
                        ui_text(26.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .line_height(px(32.0))
                            .child("No photos yet."),
                    )
                    .child(
                        ui_text(12.0, theme::muted())
                            .line_height(px(18.0))
                            .child(
                                "Drop photos anywhere, or import from the menu. Tag and rename in bulk, then lay out the sheet.",
                            ),
                    )
                    .child(rule())
                    .child(
                        mono_text(theme::TEXT_MICRO, theme::faint())
                            .line_height(px(15.0))
                            .child("JPEG · PNG · TIFF · WebP · HEIC · RAW"),
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
                    .w(px(400.0))
                    .flex()
                    .flex_col()
                    .gap(px(14.0))
                    .child(
                        div()
                            .size(px(34.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(theme::rule())
                            .bg(theme::surface())
                            .child(icon(assets::SEARCH_X, 17.0, theme::muted())),
                    )
                    .child(
                        ui_text(24.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .line_height(px(30.0))
                            .child("No matches."),
                    )
                    .child(
                        ui_text(11.5, theme::muted())
                            .line_height(px(17.0))
                            .child("Try another file name or tag."),
                    )
                    .child(
                        button_with_icon("clear-filters-empty", assets::X, "Clear filters")
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
