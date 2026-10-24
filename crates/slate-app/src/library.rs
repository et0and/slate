//! The image library grid.

use gpui::{
    AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px, uniform_list,
};

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{button_with_icon, icon, mono_text, ui_text};

const CELL_W: f32 = 200.0;
const THUMB_H: f32 = 150.0;
const GAP: f32 = 16.0;
const ROW_H: f32 = 236.0;
const GRID_PAD: f32 = 24.0;

impl ContactSheet {
    pub(crate) fn render_library(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let visible = self.visible_indices();
        let body: AnyElement = if self.project.photos.is_empty() {
            self.render_empty_state(cx)
        } else if visible.is_empty() {
            self.render_no_matches(cx)
        } else {
            let viewport = f32::from(window.viewport_size().width);
            let center = (viewport - self.panels.left() - self.panels.right() - 16.0)
                .max(CELL_W + GRID_PAD * 2.0);
            let content = (center - GRID_PAD * 2.0).max(CELL_W);
            let columns = (((content + GAP) / (CELL_W + GAP)).floor() as usize).max(1);
            let rows = visible.len().div_ceil(columns);
            let visible_for_grid = visible.clone();
            let scroll = self.grid_scroll.clone();
            let list = uniform_list(
                "library-grid",
                rows,
                cx.processor(
                    move |this: &mut ContactSheet,
                          range: std::ops::Range<usize>,
                          _: &mut Window,
                          cx: &mut Context<ContactSheet>| {
                        let mut indices = Vec::with_capacity(range.len() * columns);
                        let mut rendered = Vec::with_capacity(range.len());
                        for row in range {
                            let start = row * columns;
                            let end = (start + columns).min(visible_for_grid.len());
                            let mut cells = Vec::with_capacity(end.saturating_sub(start));
                            for &index in visible_for_grid.iter().take(end).skip(start) {
                                indices.push(index);
                                cells.push(this.render_cell(index, cx));
                            }
                            rendered.push(
                                div()
                                    .w_full()
                                    .h(px(ROW_H))
                                    .flex()
                                    .gap(px(GAP))
                                    .children(cells),
                            );
                        }
                        this.schedule_visible_thumbs(indices, cx);
                        rendered
                    },
                ),
            )
            .track_scroll(scroll);

            div()
                .id("library-grid-wrap")
                .flex_1()
                .min_h(px(0.0))
                .p(px(GRID_PAD))
                .child(list.h_full())
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

    fn render_cell(&mut self, index: usize, cx: &mut Context<Self>) -> AnyElement {
        let (id, name, dimensions, tags, bytes, rotation) = {
            let photo = &self.project.photos[index];
            (
                photo.id(),
                photo.file_name(),
                photo
                    .oriented_size()
                    .map(|(width, height)| format!("{width} × {height}"))
                    .unwrap_or_else(|| "—".to_string()),
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
            .top(px(10.0))
            .right(px(38.0))
            .size(px(22.0))
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
            .child(icon(assets::ROTATE, 13.0, theme::muted()));

        let badge = selected.then(|| {
            div()
                .absolute()
                .top(px(10.0))
                .right(px(10.0))
                .size(px(22.0))
                .bg(theme::accent())
                .shadow_sm()
                .flex()
                .items_center()
                .justify_center()
                .child(icon(assets::CHECK, 13.0, theme::accent_text()))
        });

        div()
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
                            .line_height(px(19.0))
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
                            .child(mono_text(theme::TEXT_MICRO, theme::faint()).child(bytes)),
                    )
                    .children(tags.map(|tags| {
                        ui_text(theme::TEXT_MICRO, theme::muted())
                            .line_height(px(14.0))
                            .truncate()
                            .child(tags)
                    })),
            )
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
                    .w_full()
                    .max_w(px(460.0))
                    .flex()
                    .flex_col()
                    .gap(px(8.0))
                    .child(
                        ui_text(26.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .line_height(px(32.0))
                            .child("Add some photos here"),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY, theme::muted())
                            .line_height(px(20.0))
                            .child("Drag and drop or import from a folder"),
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
                            .size(px(36.0))
                            .flex()
                            .items_center()
                            .justify_center()
                            .border_1()
                            .border_color(theme::rule())
                            .bg(theme::surface())
                            .child(icon(assets::SEARCH_X, 18.0, theme::muted())),
                    )
                    .child(
                        ui_text(24.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .line_height(px(30.0))
                            .child("No matches."),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY, theme::muted())
                            .line_height(px(19.0))
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
