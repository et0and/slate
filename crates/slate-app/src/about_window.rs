//! Native About window.

use gpui::{Context, FocusHandle, Focusable, Render, Window, div, prelude::*, px};

use crate::app::Dismiss;
use crate::theme;
use crate::widgets::{WordmarkGrid, mono_text, pixel_wordmark, ui_text};

const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";

/// The wordmark's height; the mark beside it is sized from the same grid.
const LOGO_HEIGHT: f32 = 22.0;
/// The mark's blocks are this much wider than tall.
const MARK_ASPECT: f32 = 1.4;

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
        // Size both halves of the logo in whole device pixels, from one grid,
        // so the mark's gaps match the wordmark's and nothing blurs.
        let scale = window.scale_factor();
        let grid = WordmarkGrid::fit_height(LOGO_HEIGHT, scale);
        let gap = grid.gap();
        let block_h = ((grid.height() - gap) / 2.0 * scale).floor() / scale;
        let block_w = (block_h * MARK_ASPECT * scale).round() / scale;
        let block = || div().w(px(block_w)).h(px(block_h)).bg(theme::ink());
        let mark_row = || {
            div()
                .flex()
                .gap(px(gap))
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
                                    .gap(px(gap))
                                    .child(mark_row())
                                    .child(mark_row()),
                            )
                            .child(pixel_wordmark(grid, theme::ink())),
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
