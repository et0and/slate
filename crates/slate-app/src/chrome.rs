//! Window chrome: primary commands and status.

use gpui::{AnyElement, Context, FontWeight, div, prelude::*, px};

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::ui_text;

impl ContactSheet {
    pub(crate) fn render_statusbar(&self) -> AnyElement {
        let busy = self
            .import_state
            .as_ref()
            .map(|state| format!("Importing {}/{}", state.done, state.total));
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
                div()
                    .flex()
                    .flex_1()
                    .min_w(px(0.0))
                    .items_center()
                    .gap(px(9.0))
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .font_weight(FontWeight::SEMIBOLD)
                            .text_size(px(theme::TEXT_BODY))
                            .text_color(if self.exporting.is_some() {
                                theme::muted()
                            } else {
                                theme::ink()
                            })
                            .child(">"),
                    )
                    .child(
                        ui_text(theme::TEXT_META, theme::muted())
                            .truncate()
                            .child(left),
                    ),
            )
            .child(ui_text(theme::TEXT_META, theme::faint()).child(saved))
            .into_any_element()
    }

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
