//! Window chrome: primary commands and status.

use gpui::{AnyElement, Context, div, prelude::*, px};

use crate::app::ContactSheet;
use crate::assets;
use crate::panels::{RAIL_W, ResizeTarget};
use crate::theme;
use crate::widgets::{icon, ui_text};

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
            .child(ui_text(theme::TEXT_META, theme::faint()).child(saved))
            .into_any_element()
    }

    pub(crate) fn render_collapsed_rail(
        &mut self,
        side: ResizeTarget,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let (id, icon_path, side_left): (&'static str, &'static str, bool) = match side {
            ResizeTarget::Left => ("rail-expand-left", assets::CHEVRON_RIGHT, true),
            ResizeTarget::Right => ("rail-expand-right", assets::CHEVRON_LEFT, false),
        };
        let top = match side {
            ResizeTarget::Left => theme::WINDOW_CHROME_TOP + 8.0,
            ResizeTarget::Right => 8.0,
        };
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
        rail.child(
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
                .child(icon(icon_path, 14.0, theme::muted())),
        )
        .into_any_element()
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
            .into_any_element()
    }
}
