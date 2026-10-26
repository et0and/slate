//! Native About window.

use gpui::{Context, FocusHandle, Focusable, Render, Window, div, prelude::*, px};

use crate::app::Dismiss;
use crate::assets;
use crate::theme;
use crate::widgets::{button, heading, icon, mono_text, ui_text};

const LICENSE_URL: &str = "https://www.gnu.org/licenses/gpl-3.0.html";

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
            .on_action(cx.listener(|_, _: &Dismiss, window, _| window.remove_window()))
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
                    .child(heading("Slate"))
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .child(format!("Version {version}")),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY, theme::muted())
                            .line_height(px(20.0))
                            .child("Review, rate and rename photographs, then print them as contact sheets."),
                    )
                    .child(div().h(px(1.0)).w_full().bg(theme::rule()))
                    .child(ui_text(theme::TEXT_BODY, theme::ink()).child("Tom Hackshaw"))
                    .child(
                        div()
                            .id("about-license")
                            .flex()
                            .items_center()
                            .gap(px(6.0))
                            .cursor_pointer()
                            .hover(|style| style.opacity(0.72))
                            .on_click(|_, _, cx| cx.open_url(LICENSE_URL))
                            .child(
                                ui_text(theme::TEXT_BODY, theme::muted())
                                    .underline()
                                    .child("GNU General Public License v3.0"),
                            )
                            .child(icon(assets::EXTERNAL_LINK, 12.0, theme::faint())),
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
