//! Native About window.

use gpui::{Context, FocusHandle, Focusable, Render, Window, div, prelude::*, px};

use crate::app::Dismiss;
use crate::theme;
use crate::widgets::{mono_text, pixel_wordmark, ui_text, wordmark_width};

const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";

/// The mark is three columns by two rows of blocks, as tall as the wordmark.
const MARK_BLOCK_W: f32 = 14.0;
const MARK_BLOCK_H: f32 = 10.0;
const MARK_GAP: f32 = 2.0;

pub struct AboutWindow {
    focus: FocusHandle,
}

impl AboutWindow {
    pub fn new(cx: &mut Context<Self>) -> Self {
        Self {
            focus: cx.focus_handle(),
        }
    }
}

impl Focusable for AboutWindow {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for AboutWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let version = env!("CARGO_PKG_VERSION");
        let mark_height = MARK_BLOCK_H.mul_add(2.0, MARK_GAP);
        let block = || {
            div()
                .w(px(MARK_BLOCK_W))
                .h(px(MARK_BLOCK_H))
                .bg(theme::ink())
        };
        let mark_row = || {
            div()
                .flex()
                .gap(px(MARK_GAP))
                .child(block())
                .child(block())
                .child(block())
        };

        div()
            .track_focus(&self.focus)
            .on_action(cx.listener(|_, _: &Dismiss, window, _| window.remove_window()))
            .size_full()
            .flex()
            .items_center()
            .justify_center()
            .bg(theme::paper())
            .text_color(theme::ink())
            .font_family(theme::FONT_UI)
            .child(
                div()
                    .flex()
                    .flex_col()
                    .items_start()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(8.0))
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap(px(MARK_GAP))
                                    .child(mark_row())
                                    .child(mark_row()),
                            )
                            .child(pixel_wordmark(wordmark_width(mark_height), theme::ink())),
                    )
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .mt(px(12.0))
                            .child(format!("Version {version}")),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY, theme::ink())
                            .mt(px(14.0))
                            .child("A workspace for photographs, by Yufugumi and family"),
                    )
                    .child(
                        div()
                            .id("about-license")
                            .mt(px(2.0))
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.72))
                            .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
                            .child(
                                ui_text(theme::TEXT_BODY, theme::muted())
                                    .underline()
                                    .child("License"),
                            ),
                    ),
            )
    }
}
