//! Native About window.

use gpui::{Context, FocusHandle, Focusable, Render, Window, div, prelude::*, px};

use crate::theme;
use crate::widgets::{button, mono_text, ui_text};

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

        div()
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::paper())
            .text_color(theme::ink())
            .font_family(theme::FONT_UI)
            .child(
                div()
                    .flex_1()
                    .px(px(28.0))
                    .pt(px(28.0))
                    .flex()
                    .flex_col()
                    .gap(px(12.0))
                    .child(
                        ui_text(24.0, theme::ink())
                            .font_weight(gpui::FontWeight::SEMIBOLD)
                            .child("Slate Desktop"),
                    )
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .child(format!("Version {version}")),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY, theme::muted())
                            .line_height(px(20.0))
                            .child("A monochrome contact-sheet builder for photographers."),
                    )
                    .child(div().h(px(1.0)).w_full().bg(theme::rule()))
                    .child(
                        ui_text(theme::TEXT_LABEL, theme::muted())
                            .line_height(px(18.0))
                            .child("Built with GPUI and Rust."),
                    )
                    .child(
                        ui_text(theme::TEXT_LABEL, theme::muted())
                            .line_height(px(18.0))
                            .child("Geist and Geist Mono, SIL Open Font License."),
                    )
                    .child(
                        ui_text(theme::TEXT_LABEL, theme::muted())
                            .line_height(px(18.0))
                            .child("Sample photographs from Unsplash."),
                    )
                    .child(
                        ui_text(theme::TEXT_LABEL, theme::muted())
                            .line_height(px(18.0))
                            .child("GPLv3 License."),
                    ),
            )
            .child(
                div()
                    .px(px(28.0))
                    .py(px(16.0))
                    .flex()
                    .justify_end()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(
                        button("about-native-close", "Close")
                            .on_click(cx.listener(|_, _, window, _| window.remove_window())),
                    ),
            )
    }
}
