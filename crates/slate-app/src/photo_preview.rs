//! Native full-screen photo preview window.

use gpui::{
    AnyElement, Context, FocusHandle, Focusable, MouseButton, MouseDownEvent, MouseMoveEvent,
    MouseUpEvent, Pixels, Point, Render, Window, actions, div, img, point, prelude::*, px, rgb,
};
use slate_core::model::PhotoId;

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::icon;

actions!(
    photo_preview_actions,
    [PreviousPhoto, NextPhoto, ClosePhotoPreview]
);

pub struct PhotoPreviewWindow {
    parent: gpui::Entity<ContactSheet>,
    focus: FocusHandle,
    zoom: f32,
    pan: Point<Pixels>,
    drag_start: Option<(Point<Pixels>, Point<Pixels>)>,
    shown_target: Option<PhotoId>,
}

impl PhotoPreviewWindow {
    pub fn new(parent: gpui::Entity<ContactSheet>, cx: &mut Context<Self>) -> Self {
        cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        Self {
            parent,
            focus: cx.focus_handle(),
            zoom: 1.0,
            pan: point(px(0.0), px(0.0)),
            drag_start: None,
            shown_target: None,
        }
    }
}

impl Focusable for PhotoPreviewWindow {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PhotoPreviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let (target, image) = {
            let parent = self.parent.read(cx);
            (
                parent.photo_preview_target,
                parent.photo_preview_image.clone(),
            )
        };
        if target != self.shown_target {
            self.zoom = 1.0;
            self.pan = point(px(0.0), px(0.0));
            self.drag_start = None;
            self.shown_target = target;
        }

        let viewport = window.viewport_size();
        let (image_width, image_height) = image.as_ref().map_or((1.0, 1.0), |image| {
            let size = image.size(0);
            (size.width.0.max(1) as f32, size.height.0.max(1) as f32)
        });
        let fit = ((f32::from(viewport.width) - 48.0) / image_width)
            .min((f32::from(viewport.height) - 48.0) / image_height);
        let width = image_width * fit * self.zoom;
        let height = image_height * fit * self.zoom;
        let max_x = ((width - f32::from(viewport.width)) / 2.0).max(0.0);
        let max_y = ((height - f32::from(viewport.height)) / 2.0).max(0.0);
        self.pan = point(
            px(f32::from(self.pan.x).clamp(-max_x, max_x)),
            px(f32::from(self.pan.y).clamp(-max_y, max_y)),
        );
        let left = (f32::from(viewport.width) - width) / 2.0 + f32::from(self.pan.x);
        let top = (f32::from(viewport.height) - height) / 2.0 + f32::from(self.pan.y);

        let body: AnyElement = image.map_or_else(
            || div().size_full().into_any_element(),
            |image| {
                div()
                    .absolute()
                    .top(px(top))
                    .left(px(left))
                    .w(px(width))
                    .h(px(height))
                    .child(img(image).object_fit(gpui::ObjectFit::Fill).size_full())
                    .into_any_element()
            },
        );

        let close = div()
            .id("photo-preview-close")
            .size(px(34.0))
            .flex()
            .items_center()
            .justify_center()
            .border_1()
            .border_color(rgba_white(0.22))
            .bg(rgba_white(0.06))
            .cursor_pointer()
            .hover(|style| style.bg(rgba_white(0.14)))
            .on_click(cx.listener(|this, _, window, cx| {
                this.parent.update(cx, ContactSheet::clear_photo_preview);
                window.remove_window();
            }))
            .child(icon(assets::X, 16.0, rgb(0xf3f3f1)));

        let previous = preview_button("photo-preview-prev", assets::CHEVRON_LEFT).on_click(
            cx.listener(|this, _, _, cx| {
                this.parent
                    .update(cx, |parent, cx| parent.step_photo_preview(-1, cx));
            }),
        );
        let next = preview_button("photo-preview-next", assets::CHEVRON_RIGHT).on_click(
            cx.listener(|this, _, _, cx| {
                this.parent
                    .update(cx, |parent, cx| parent.step_photo_preview(1, cx));
            }),
        );

        let controls = div()
            .absolute()
            .top(px(18.0))
            .right(px(18.0))
            .flex()
            .items_center()
            .gap(px(8.0))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|_, _, _, cx| cx.stop_propagation()),
            )
            .child(previous)
            .child(next)
            .child(close);

        div()
            .key_context("PhotoPreviewWindow")
            .track_focus(&self.focus)
            .size_full()
            .relative()
            .overflow_hidden()
            .bg(rgb(0x050505))
            .text_color(rgb(0xf3f3f1))
            .font_family(theme::FONT_UI)
            .on_action(cx.listener(|this, _: &PreviousPhoto, _, cx| {
                this.parent
                    .update(cx, |parent, cx| parent.step_photo_preview(-1, cx));
            }))
            .on_action(cx.listener(|this, _: &NextPhoto, _, cx| {
                this.parent
                    .update(cx, |parent, cx| parent.step_photo_preview(1, cx));
            }))
            .on_action(cx.listener(|this, _: &ClosePhotoPreview, window, cx| {
                this.parent.update(cx, ContactSheet::clear_photo_preview);
                window.remove_window();
            }))
            .on_scroll_wheel(cx.listener(|this, event: &gpui::ScrollWheelEvent, _, cx| {
                let delta = f32::from(event.delta.pixel_delta(px(24.0)).y);
                let next = (this.zoom * (delta / 300.0).exp()).clamp(1.0, 8.0);
                if (next - this.zoom).abs() > f32::EPSILON {
                    this.zoom = next;
                    if next <= 1.0 {
                        this.pan = point(px(0.0), px(0.0));
                    }
                    cx.notify();
                }
                cx.stop_propagation();
            }))
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(|this, event: &MouseDownEvent, _, cx| {
                    if this.zoom > 1.0 {
                        this.drag_start = Some((event.position, this.pan));
                        cx.stop_propagation();
                    }
                }),
            )
            .on_mouse_move(cx.listener(|this, event: &MouseMoveEvent, _, cx| {
                let Some((start, pan)) = this.drag_start else {
                    return;
                };
                if !event.dragging() {
                    return;
                }
                this.pan = pan + (event.position - start);
                cx.notify();
            }))
            .on_mouse_up(
                MouseButton::Left,
                cx.listener(|this, _: &MouseUpEvent, _, _| {
                    this.drag_start = None;
                }),
            )
            .child(body)
            .child(controls)
    }
}

fn preview_button(id: &'static str, path: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .size(px(34.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(rgba_white(0.22))
        .bg(rgba_white(0.06))
        .cursor_pointer()
        .hover(|style| style.bg(rgba_white(0.14)))
        .child(icon(path, 15.0, rgb(0xf3f3f1)))
}

fn rgba_white(alpha: f32) -> gpui::Rgba {
    gpui::rgba(0xffffff00 | ((alpha.clamp(0.0, 1.0) * 255.0) as u32))
}
