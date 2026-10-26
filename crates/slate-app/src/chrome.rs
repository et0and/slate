//! Window chrome: primary commands and status.

use gpui::{AnyElement, Context, div, prelude::*, px};
#[cfg(target_os = "macos")]
use gpui::{ClickEvent, MouseButton};

use crate::app::ContactSheet;
use crate::assets;
#[cfg(target_os = "macos")]
use crate::panels::DIVIDER_W;
use crate::panels::{RAIL_W, ResizeTarget};
use crate::theme;
#[cfg(not(target_os = "macos"))]
use crate::widgets::icon_button;
use crate::widgets::{icon, mono_text, ui_text};

impl ContactSheet {
    pub(crate) fn render_statusbar(&self) -> AnyElement {
        let busy = self
            .drive_download
            .as_ref()
            .map(|state| format!("Downloading {} from Google Drive", state.total))
            .or_else(|| {
                self.import_state
                    .as_ref()
                    .map(|state| format!("Importing {}/{}", state.done, state.total))
            });
        let left = busy.unwrap_or_else(|| self.status.clone());
        let saved = if self.dirty {
            "Unsaved changes"
        } else if self.project_path.is_some() {
            "Saved"
        } else {
            "Not saved"
        };

        div()
            .h(px(theme::STATUS_H))
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::rule())
            .bg(theme::panel())
            .flex()
            .items_center()
            .justify_between()
            .px(px(18.0))
            .child(
                div().flex().flex_1().min_w(px(0.0)).items_center().child(
                    ui_text(
                        theme::TEXT_LABEL,
                        if self.exporting.is_some() {
                            theme::muted()
                        } else {
                            theme::ink()
                        },
                    )
                    .truncate()
                    .child(left),
                ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(12.0))
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .child(format!("{} selected", self.selected.len())),
                    )
                    .child(
                        mono_text(theme::TEXT_META, theme::faint())
                            .child(format!("v{}", env!("CARGO_PKG_VERSION"))),
                    )
                    .child(ui_text(theme::TEXT_META, theme::faint()).child(saved)),
            )
            .into_any_element()
    }

    pub(crate) fn render_collapsed_rail(
        &mut self,
        side: ResizeTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, icon_path, side_left): (&'static str, &'static str, bool) = match side {
            ResizeTarget::Left => ("rail-expand-left", assets::PANEL_LEFT, true),
            ResizeTarget::Right => ("rail-expand-right", assets::PANEL_RIGHT, false),
        };
        let top = 8.0;
        let rail = div()
            .w(px(RAIL_W))
            .h_full()
            .flex_shrink_0()
            .flex()
            .flex_col()
            .items_center()
            .pt(px(top))
            .bg(theme::panel());
        let rail = match side {
            ResizeTarget::Left => rail.border_r_1().border_color(theme::rule()),
            ResizeTarget::Right => rail.border_l_1().border_color(theme::rule()),
        };
        let show_rail_button = !(side_left && cfg!(target_os = "macos"));
        rail.children(show_rail_button.then(|| {
            div()
                .id(id)
                .w_full()
                .h(px(32.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .text_color(theme::muted())
                .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
                .on_click(cx.listener(move |this, _, window, cx| {
                    if side_left {
                        this.toggle_left_pane(window, cx);
                    } else {
                        this.toggle_right_pane(window, cx);
                    }
                }))
                .child(icon(icon_path, 14.0, theme::muted()))
        }))
        .into_any_element()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn render_window_titlebar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let left_width = if self.panels.left_collapsed() {
            112.0
        } else {
            self.panels.left()
        };
        let right_width = if self.panels.right_collapsed() {
            RAIL_W
        } else {
            self.panels.right()
        };
        let left = div()
            .w(px(left_width))
            .h_full()
            .flex_shrink_0()
            .bg(theme::panel())
            .flex()
            .items_center()
            .child(div().w(px(78.0)).flex_shrink_0())
            .child(
                div()
                    .id("titlebar-collapse-left")
                    .size(px(26.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.opacity(0.72))
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.toggle_left_pane(window, cx);
                    }))
                    .child(icon(
                        if self.panels.left_collapsed() {
                            assets::PANEL_RIGHT
                        } else {
                            assets::PANEL_LEFT
                        },
                        15.0,
                        theme::muted(),
                    )),
            )
            .child(drag_region(cx, "titlebar-drag-left").flex_1().h_full());
        let mut right = div()
            .w(px(right_width))
            .h_full()
            .flex_shrink_0()
            .bg(theme::panel())
            .child(drag_region(cx, "titlebar-drag-right").w_full().h_full());
        if self.panels.right_collapsed() {
            right = right.border_l_1().border_color(theme::rule());
        }
        let mut row = div()
            .h(px(theme::WINDOW_CHROME_TOP))
            .w_full()
            .flex_shrink_0()
            .flex()
            .bg(theme::paper())
            .child(left);
        if !self.panels.left_collapsed() {
            row = row.child(
                div()
                    .w(px(DIVIDER_W))
                    .h_full()
                    .flex_shrink_0()
                    .bg(theme::paper())
                    .flex()
                    .justify_center()
                    .child(div().w(px(1.0)).h_full().bg(theme::rule())),
            );
        }
        row = row.child(drag_region(cx, "titlebar-drag-centre").flex_1().h_full());
        if !self.panels.right_collapsed() {
            row = row.child(
                div()
                    .w(px(DIVIDER_W))
                    .h_full()
                    .flex_shrink_0()
                    .bg(theme::paper())
                    .flex()
                    .justify_center()
                    .child(div().w(px(1.0)).h_full().bg(theme::rule())),
            );
        }
        row.child(right).into_any_element()
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn render_window_titlebar(&mut self, _cx: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    #[cfg(target_os = "macos")]
    pub(crate) fn render_topbar(&mut self, _cx: &mut Context<Self>) -> AnyElement {
        div().into_any_element()
    }

    #[cfg(not(target_os = "macos"))]
    pub(crate) fn render_topbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .h(px(theme::TOPBAR_H))
            .flex_shrink_0()
            .flex()
            .items_center()
            .px(px(8.0))
            .bg(theme::paper())
            .border_b_1()
            .border_color(theme::rule())
            .shadow_sm()
            .child(self.render_menubar(cx))
            .child(div().flex_1())
            .children(self.panels.left_collapsed().then(|| {
                icon_button("topbar-expand-left", assets::PANEL_RIGHT)
                    .on_click(cx.listener(|this, _, window, cx| this.toggle_left_pane(window, cx)))
            }))
            .into_any_element()
    }
}

#[cfg(target_os = "macos")]
fn drag_region(cx: &mut Context<ContactSheet>, id: &'static str) -> gpui::Stateful<gpui::Div> {
    div()
        .id(id)
        .cursor_pointer()
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _, window, _| window.start_window_move()),
        )
        .on_click(cx.listener(|_, event: &ClickEvent, window, _| {
            if event.click_count() == 2 {
                window.titlebar_double_click();
            }
        }))
}
