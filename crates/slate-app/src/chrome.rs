//! Window chrome: masthead with project state and primary commands.

use gpui::{AnyElement, Context, div, prelude::*, px};

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, micro, primary_button};

impl ContactSheet {
    pub(crate) fn render_statusbar(&self) -> AnyElement {
        let busy = self
            .import_state
            .as_ref()
            .map(|state| format!("IMPORTING {}/{}", state.done, state.total));
        let left = busy.unwrap_or_else(|| self.status.clone());

        div()
            .h(px(26.0))
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::ink())
            .bg(theme::panel())
            .flex()
            .items_center()
            .justify_between()
            .px(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(7.0))
                    .child(div().w(px(6.0)).h(px(6.0)).bg(if self.exporting.is_some() {
                        theme::accent()
                    } else {
                        theme::ink()
                    }))
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::muted())
                            .truncate()
                            .child(left.to_uppercase()),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(14.0))
                    .child(micro(if self.dirty {
                        "PROJECT UNSAVED"
                    } else if self.project_path.is_some() {
                        "PROJECT SAVED"
                    } else {
                        "PROJECT NOT SAVED"
                    }))
                    .child(micro("CMD+S SAVE / CMD+O OPEN / CMD+E EXPORT")),
            )
            .into_any_element()
    }

    pub(crate) fn render_topbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let wordmark = div()
            .w(px(252.0))
            .h_full()
            .flex_shrink_0()
            .border_r_1()
            .border_color(theme::rule())
            .px(px(12.0))
            .flex()
            .items_center()
            .gap(px(9.0))
            .child(
                div()
                    .w(px(9.0))
                    .h(px(9.0))
                    .bg(theme::accent())
                    .flex_shrink_0(),
            )
            .child(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(1.0))
                    .child(
                        div()
                            .text_size(px(14.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .child("SLATE"),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::faint())
                            .child("CONTACT SHEET SYSTEM / V0.1"),
                    ),
            );

        let project = div()
            .flex_1()
            .min_w(px(0.0))
            .px(px(14.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(12.0))
                    .font_weight(gpui::FontWeight::BOLD)
                    .truncate()
                    .child(self.project.name.to_uppercase()),
            )
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(8.0))
                    .text_color(theme::faint())
                    .truncate()
                    .child(
                        self.project_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "UNSAVED PROJECT / IMPORT TO BEGIN".to_string()),
                    ),
            );

        let stats = div()
            .flex()
            .items_center()
            .gap(px(18.0))
            .px(px(14.0))
            .child(stat("PHOTOS", self.project.photos.len()))
            .child(stat("SELECTED", self.selected.len()))
            .child(stat("SHEETS", self.project.saved_sheets.len()))
            .child(stat("PAGES", self.display_page_count()));

        let actions = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(12.0))
            .h_full()
            .border_l_1()
            .border_color(theme::rule())
            .child(
                button("top-open", "OPEN")
                    .on_click(cx.listener(|this, _, _, cx| this.open_project_prompt(cx))),
            )
            .child(
                button("top-save", if self.dirty { "SAVE *" } else { "SAVE" })
                    .on_click(cx.listener(|this, _, _, cx| this.save_project(cx))),
            )
            .child(
                primary_button("top-export", "EXPORT PDF")
                    .h(px(24.0))
                    .on_click(cx.listener(|this, _, _, cx| this.export_current(cx))),
            );

        div()
            .h(px(54.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(theme::paper())
            .border_b_1()
            .border_color(theme::ink())
            .child(wordmark)
            .child(project)
            .child(stats)
            .child(actions)
            .into_any_element()
    }
}

fn stat(name: &str, value: usize) -> AnyElement {
    div()
        .flex()
        .flex_col()
        .gap(px(1.0))
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(8.0))
                .text_color(theme::faint())
                .child(name.to_string()),
        )
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(10.0))
                .text_color(theme::ink())
                .child(format!("{value:04}")),
        )
        .into_any_element()
}
