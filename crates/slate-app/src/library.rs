//! The image library grid.

use std::sync::Arc;

use gpui::{
    AnyElement, Context, ElementId, MouseButton, MouseDownEvent, MouseMoveEvent, Pixels, Point,
    SharedString, Window, div, img, prelude::*, px, uniform_list,
};
use slate_core::model::PhotoId;

use crate::app::{ContactSheet, Marquee};
use crate::assets;

use crate::theme;
use crate::widgets::{button_with_icon, icon, mono_text, ui_text};

const CELL_MIN: f32 = 190.0;
const GAP: f32 = 16.0;
const GRID_PAD: f32 = 24.0;
const META_H: f32 = 86.0;
const THUMB_RATIO: f32 = 0.75;

#[derive(Clone, Copy, Debug)]
struct GridMetrics {
    columns: usize,
    cell_w: f32,
    thumb_h: f32,
    row_h: f32,
}

fn grid_metrics(viewport: f32, left_edge: f32, right_edge: f32) -> GridMetrics {
    let center = (viewport - left_edge - right_edge).max(CELL_MIN + GRID_PAD * 2.0);
    let content = (center - GRID_PAD * 2.0).max(CELL_MIN);
    let columns = (((content + GAP) / (CELL_MIN + GAP)).floor() as usize).max(1);
    let cell_w = ((content - GAP * (columns.saturating_sub(1)) as f32) / columns as f32).max(1.0);
    let thumb_h = (cell_w * THUMB_RATIO).max(120.0);
    GridMetrics {
        columns,
        cell_w,
        thumb_h,
        row_h: thumb_h + META_H,
    }
}

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
            let metrics = grid_metrics(
                f32::from(window.viewport_size().width),
                self.panels.left_edge(),
                self.panels.right_edge(),
            );
            let columns = metrics.columns;
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
                                cells.push(this.render_cell(index, metrics, cx));
                            }
                            rendered.push(
                                div()
                                    .w_full()
                                    .h(px(metrics.row_h))
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
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, event: &MouseDownEvent, _, cx| {
                        this.begin_marquee(None, event.position, cx);
                    }),
                )
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

    fn render_cell(
        &mut self,
        index: usize,
        metrics: GridMetrics,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, name, dimensions, tags, bytes, rotation, rating, color_label) = {
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
                photo.rating(),
                photo.label(),
            )
        };
        let selected = self.selected.contains(id);
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

        let eye = div()
            .id(ElementId::Name(SharedString::from(format!(
                "preview-{}",
                id.0
            ))))
            .absolute()
            .top(px(10.0))
            .left(px(10.0))
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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .debug_selector(move || format!("preview-{}", id.0))
            .on_click(cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                this.open_photo_preview(id, window, cx);
            }))
            .child(icon(assets::EYE, 13.0, theme::muted()));

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
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
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
            .w(px(metrics.cell_w))
            .p(px(5.0))
            .cursor_pointer()
            .hover(|style| style.bg(theme::accent_soft()))
            .active(|style| style.opacity(0.82))
            .debug_selector(move || format!("cell-{}", id.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.begin_marquee(Some(id), event.position, cx);
                    this.inspect_photo(id, cx);
                }),
            )
            .on_mouse_down(
                MouseButton::Right,
                cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                    cx.stop_propagation();
                    this.open_photo_context(id, event.position, cx);
                }),
            )
            .child(
                div()
                    .relative()
                    .w_full()
                    .h(px(metrics.thumb_h))
                    .overflow_hidden()
                    .bg(theme::photo_bg())
                    .border_1()
                    .border_color(theme::rule())
                    .shadow_sm()
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(preview)
                    .child(eye)
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
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(1.0))
                                    .children((1..=5).map(|value| {
                                        icon(
                                            if rating.get() >= value {
                                                assets::STAR_FILLED
                                            } else {
                                                assets::STAR
                                            },
                                            9.0,
                                            if rating.get() >= value {
                                                theme::muted()
                                            } else {
                                                theme::rule()
                                            },
                                        )
                                    })),
                            )
                            .child(
                                div()
                                    .size(px(9.0))
                                    .border_1()
                                    .border_color(theme::rule())
                                    .bg(if color_label == slate_core::model::ColorLabel::None {
                                        theme::surface()
                                    } else {
                                        theme::label_color(color_label)
                                    }),
                            ),
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

    pub(crate) fn begin_marquee(
        &mut self,
        photo_id: Option<PhotoId>,
        position: Point<Pixels>,
        cx: &mut Context<Self>,
    ) {
        let target = photo_id.is_none_or(|id| !self.selected.contains(id));
        let mut changed = false;
        if let Some(photo_id) = photo_id {
            let selected = Arc::make_mut(&mut self.selected);
            changed = if target {
                selected.insert(photo_id)
            } else {
                selected.remove(photo_id)
            };
        }
        self.marquee = Some(Marquee {
            start: position,
            current: position,
            target,
            moved: false,
        });
        if changed {
            self.selection_changed();
            self.refresh_preview();
        }
        cx.notify();
    }

    pub(crate) fn marquee_move(
        &mut self,
        event: &MouseMoveEvent,
        window: &Window,
        cx: &mut Context<Self>,
    ) {
        let Some(marquee) = self.marquee.as_ref() else {
            return;
        };
        if !event.dragging() {
            return;
        }
        let moved = marquee.moved
            || (event.position.x - marquee.start.x).abs() > px(2.0)
            || (event.position.y - marquee.start.y).abs() > px(2.0);
        let target = marquee.target;
        self.marquee = Some(Marquee {
            start: marquee.start,
            current: event.position,
            target,
            moved,
        });
        if moved {
            self.apply_marquee_selection(window, cx);
        }
        cx.notify();
    }

    pub(crate) fn end_marquee(&mut self, cx: &mut Context<Self>) {
        if self.marquee.take().is_some() {
            cx.notify();
        }
    }

    fn apply_marquee_selection(&mut self, window: &Window, cx: &mut Context<Self>) {
        let Some((start, current, target)) = self
            .marquee
            .as_ref()
            .map(|marquee| (marquee.start, marquee.current, marquee.target))
        else {
            return;
        };
        let viewport = f32::from(window.viewport_size().width);
        let metrics = grid_metrics(viewport, self.panels.left_edge(), self.panels.right_edge());
        let columns = metrics.columns;
        let visible = self.visible_indices();
        let rows = visible.len().div_ceil(columns);
        if rows == 0 {
            return;
        }
        let left = f32::from(start.x).min(f32::from(current.x));
        let right = f32::from(start.x).max(f32::from(current.x));
        let top = f32::from(start.y).min(f32::from(current.y));
        let bottom = f32::from(start.y).max(f32::from(current.y));
        let content_top = if cfg!(target_os = "macos") {
            0.0
        } else {
            theme::TOPBAR_H
        };
        let origin_x = self.panels.left_edge() + GRID_PAD;
        let origin_y =
            content_top + GRID_PAD + f32::from(self.grid_scroll.0.borrow().base_handle.offset().y);
        let min_row = (((top - origin_y) / metrics.row_h).floor().max(0.0) as usize).min(rows);
        let max_row = (((bottom - origin_y) / metrics.row_h).ceil().max(0.0) as usize).min(rows);
        let min_col = ((left - origin_x) / (metrics.cell_w + GAP))
            .floor()
            .max(0.0) as usize;
        let max_col = (((right - origin_x) / (metrics.cell_w + GAP))
            .ceil()
            .max(0.0) as usize)
            .min(columns);
        let mut changed = false;
        let selected = Arc::make_mut(&mut self.selected);
        for row in min_row..max_row {
            for column in min_col..max_col {
                let index = row.saturating_mul(columns).saturating_add(column);
                let Some(project_index) = visible.get(index) else {
                    continue;
                };
                let id = self.project.photos[*project_index].id();
                changed |= if target {
                    selected.insert(id)
                } else {
                    selected.remove(id)
                };
            }
        }
        if changed {
            self.selection_changed();
            self.refresh_preview();
            cx.notify();
        }
    }

    pub(crate) fn render_marquee(&self) -> Option<AnyElement> {
        let marquee = self.marquee.as_ref()?;
        if !marquee.moved {
            return None;
        }
        let left = f32::from(marquee.start.x).min(f32::from(marquee.current.x));
        let top = f32::from(marquee.start.y).min(f32::from(marquee.current.y));
        let width = (f32::from(marquee.start.x) - f32::from(marquee.current.x)).abs();
        let height = (f32::from(marquee.start.y) - f32::from(marquee.current.y)).abs();
        Some(
            div()
                .absolute()
                .top(px(top))
                .left(px(left))
                .w(px(width))
                .h(px(height))
                .border_1()
                .border_color(theme::accent())
                .bg(theme::accent_soft())
                .opacity(0.55)
                .into_any_element(),
        )
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grid_cells_fill_the_available_width() {
        let metrics = grid_metrics(1440.0, 280.0, 384.0);
        let width = metrics.cell_w * metrics.columns as f32
            + GAP * metrics.columns.saturating_sub(1) as f32;
        let content = 1440.0 - 280.0 - 384.0 - GRID_PAD * 2.0;
        assert!((width - content).abs() < 0.01, "{width} != {content}");
    }
}
