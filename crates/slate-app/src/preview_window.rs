//! Native sheet-preview window.

use gpui::{Context, FocusHandle, Focusable, Render, Window, actions, div, prelude::*, px};

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{button, mini_button, mono_text};

actions!(
    preview_window_actions,
    [PreviousPage, NextPage, ClosePreview]
);

pub struct PreviewWindow {
    parent: gpui::Entity<ContactSheet>,
    page: usize,
    focus: FocusHandle,
}

impl PreviewWindow {
    pub fn new(parent: gpui::Entity<ContactSheet>, page: usize, cx: &mut Context<Self>) -> Self {
        cx.observe(&parent, |_, _, cx| cx.notify()).detach();
        Self {
            parent,
            page,
            focus: cx.focus_handle(),
        }
    }

    fn previous(&mut self, cx: &mut Context<Self>) {
        self.page = self.page.saturating_sub(1);
        cx.notify();
    }

    fn next(&mut self, cx: &mut Context<Self>) {
        self.page = self.page.saturating_add(1);
        cx.notify();
    }
}

impl Focusable for PreviewWindow {
    fn focus_handle(&self, _: &gpui::App) -> FocusHandle {
        self.focus.clone()
    }
}

impl Render for PreviewWindow {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if !self.focus.is_focused(window) {
            window.focus(&self.focus);
        }
        let page_count = self.parent.update(cx, |parent, _| parent.page_count());
        let page = self.page.min(page_count.saturating_sub(1));
        self.page = page;
        let (paper_w, paper_h) = self.parent.read(cx).project.sheet.paper_mm();
        let viewport = window.viewport_size();
        let max_w = (f32::from(viewport.width) - 88.0).max(320.0);
        let max_h = (f32::from(viewport.height) - 156.0).max(300.0);
        let width = (max_h * paper_w / paper_h).min(max_w).clamp(320.0, 1200.0);
        let preview = self.parent.update(cx, |parent, cx| {
            parent.render_sheet_preview(width, page, false, cx)
        });
        let height = width * paper_h / paper_w;

        div()
            .key_context("PreviewWindow")
            .track_focus(&self.focus)
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::paper())
            .text_color(theme::ink())
            .font_family(theme::FONT_UI)
            .on_action(cx.listener(|this, _: &PreviousPage, _, cx| this.previous(cx)))
            .on_action(cx.listener(|this, _: &NextPage, _, cx| this.next(cx)))
            .on_action(cx.listener(|_, _: &ClosePreview, window, _| window.remove_window()))
            .child(
                div()
                    .flex_1()
                    .min_h(px(0.0))
                    .flex()
                    .flex_col()
                    .child(
                        div()
                            .id("preview-native-scroll")
                            .flex_1()
                            .min_h(px(0.0))
                            .overflow_y_scroll()
                            .px(px(24.0))
                            .pt(px(24.0))
                            .flex()
                            .justify_center()
                            .child(div().w(px(width)).h(px(height)).child(preview)),
                    )
                    .child(
                        div()
                            .flex_shrink_0()
                            .px(px(24.0))
                            .py(px(12.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .border_t_1()
                            .border_color(theme::rule())
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(4.0))
                                    .child(
                                        mini_button("preview-native-prev", assets::CHEVRON_LEFT)
                                            .on_click(
                                                cx.listener(|this, _, _, cx| this.previous(cx)),
                                            ),
                                    )
                                    .child(
                                        mono_text(theme::TEXT_META, theme::faint())
                                            .w(px(58.0))
                                            .text_align(gpui::TextAlign::Center)
                                            .child(format!("{} / {page_count}", page + 1)),
                                    )
                                    .child(
                                        mini_button("preview-native-next", assets::CHEVRON_RIGHT)
                                            .on_click(cx.listener(|this, _, _, cx| this.next(cx))),
                                    ),
                            )
                            .child(
                                button("preview-native-done", "Close").on_click(
                                    cx.listener(|_, _, window, _| window.remove_window()),
                                ),
                            ),
                    ),
            )
    }
}
