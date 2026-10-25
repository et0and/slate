//! Native full-screen photo preview window.

use gpui::{
    AnyElement, Context, FocusHandle, Focusable, Render, Window, actions, div, img, prelude::*, px,
    rgb,
};

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{icon, mono_text, ui_text};

actions!(
    photo_preview_actions,
    [PreviousPhoto, NextPhoto, ClosePhotoPreview]
);

pub struct PhotoPreviewWindow {
    parent: gpui::Entity<ContactSheet>,
    focus: FocusHandle,
}

impl PhotoPreviewWindow {
    pub fn new(parent: gpui::Entity<ContactSheet>, cx: &mut Context<Self>) -> Self {
        cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        Self {
            parent,
            focus: cx.focus_handle(),
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
        let (target, image, error) = {
            let parent = self.parent.read(cx);
            (
                parent.photo_preview_target,
                parent.photo_preview_image.clone(),
                parent.photo_preview_error.clone(),
            )
        };
        let details = target
            .and_then(|id| self.parent.read(cx).project.photo(id))
            .map_or_else(
                || ("No photo".to_string(), String::new()),
                |photo| {
                    let size = photo.oriented_size().map_or_else(
                        || "Unknown size".to_string(),
                        |(width, height)| format!("{width} × {height}"),
                    );
                    let kind = photo.extension().to_ascii_uppercase();
                    (
                        photo.file_name(),
                        format!("{size} / {kind} / {}", format_bytes(photo.bytes)),
                    )
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
                this.parent
                    .update(cx, super::app::ContactSheet::clear_photo_preview);
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

        let body: AnyElement = image.map_or_else(
            || {
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(ui_text(theme::TEXT_BODY, rgba_white(0.55)).child(
                        error.unwrap_or_else(|| "Loading high-resolution preview…".to_string()),
                    ))
                    .into_any_element()
            },
            |image| {
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .p(px(24.0))
                    .child(
                        img(image)
                            .object_fit(gpui::ObjectFit::Contain)
                            .w_full()
                            .h_full(),
                    )
                    .into_any_element()
            },
        );

        div()
            .key_context("PhotoPreviewWindow")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
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
                this.parent
                    .update(cx, super::app::ContactSheet::clear_photo_preview);
                window.remove_window();
            }))
            .child(
                div()
                    .h(px(64.0))
                    .flex_shrink_0()
                    .px(px(22.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        div()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                ui_text(theme::TEXT_BODY, rgb(0xf3f3f1))
                                    .font_weight(gpui::FontWeight::MEDIUM)
                                    .truncate()
                                    .child(details.0),
                            )
                            .child(
                                mono_text(theme::TEXT_META, rgba_white(0.55))
                                    .truncate()
                                    .child(details.1),
                            ),
                    )
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(previous)
                            .child(next)
                            .child(close),
                    ),
            )
            .child(body)
            .child(
                div()
                    .h(px(48.0))
                    .flex_shrink_0()
                    .flex()
                    .items_center()
                    .justify_center()
                    .gap(px(22.0))
                    .child(mono_text(theme::TEXT_MICRO, rgba_white(0.4)).child("← → to browse"))
                    .child(mono_text(theme::TEXT_MICRO, rgba_white(0.4)).child("Esc to close")),
            )
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

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "Unknown size".to_string()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}
