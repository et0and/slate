//! Window chrome: masthead with project state and primary commands.

use gpui::{AnyElement, Context, div, prelude::*, px};

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{button, metric, micro, primary_button, text_button};

impl ContactSheet {
    pub(crate) fn render_statusbar(&self) -> AnyElement {
        let busy = self
            .import_state
            .as_ref()
            .map(|state| format!("Importing {}/{}", state.done, state.total));
        let left = busy.unwrap_or_else(|| self.status.clone());

        div()
            .h(px(28.0))
            .flex_shrink_0()
            .border_t_1()
            .border_color(theme::rule())
            .bg(theme::panel())
            .flex()
            .items_center()
            .justify_between()
            .px(px(14.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(8.0))
                    .child(div().w(px(5.0)).h(px(5.0)).bg(if self.exporting.is_some() {
                        theme::muted()
                    } else {
                        theme::ink()
                    }))
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.5))
                            .text_color(theme::muted())
                            .truncate()
                            .child(left),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(16.0))
                    .child(
                        div()
                            .font_family(theme::FONT_UI)
                            .text_size(px(9.0))
                            .text_color(theme::faint())
                            .child(if self.dirty {
                                "Project unsaved"
                            } else if self.project_path.is_some() {
                                "Project saved"
                            } else {
                                "Project not saved"
                            }),
                    )
                    .child(micro("⌘S save   ⌘O open   ⌘E export")),
            )
            .into_any_element()
    }

    pub(crate) fn render_topbar(&mut self, cx: &mut Context<Self>) -> AnyElement {
        let project = div()
            .flex_1()
            .min_w(px(0.0))
            .pl(px(16.0))
            .pr(px(16.0))
            .flex()
            .flex_col()
            .gap(px(2.0))
            .child(
                div()
                    .text_size(px(13.0))
                    .font_weight(gpui::FontWeight::SEMIBOLD)
                    .truncate()
                    .child(self.project.name.clone()),
            )
            .child(
                div()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(9.0))
                    .text_color(theme::faint())
                    .truncate()
                    .child(
                        self.project_path
                            .as_ref()
                            .map(|path| path.display().to_string())
                            .unwrap_or_else(|| "Unsaved project".to_string()),
                    ),
            );

        let stats = div()
            .flex()
            .items_center()
            .gap(px(22.0))
            .px(px(16.0))
            .child(metric("Photos", format!("{}", self.project.photos.len())))
            .child(metric("Selected", format!("{}", self.selected.len())))
            .child(metric(
                "Sheets",
                format!("{}", self.project.saved_sheets.len()),
            ))
            .child(metric("Pages", format!("{}", self.display_page_count())));

        let actions = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .px(px(14.0))
            .h_full()
            .border_l_1()
            .border_color(theme::rule())
            .child(
                text_button(
                    "top-appearance",
                    if self.dark { "Light mode" } else { "Dark mode" },
                )
                .on_click(cx.listener(|this, _, _, cx| this.toggle_appearance(cx))),
            )
            .child(div().w(px(1.0)).h(px(16.0)).bg(theme::rule()).mx(px(3.0)))
            .child(
                text_button("top-open", "Open")
                    .on_click(cx.listener(|this, _, _, cx| this.open_project_prompt(cx))),
            )
            .child(
                button("top-save", if self.dirty { "Save •" } else { "Save" })
                    .on_click(cx.listener(|this, _, _, cx| this.save_project(cx))),
            )
            .child(
                primary_button("top-export", "Export PDF")
                    .on_click(cx.listener(|this, _, _, cx| this.export_current(cx))),
            );

        div()
            .h(px(56.0))
            .flex_shrink_0()
            .flex()
            .items_center()
            .bg(theme::paper())
            .border_b_1()
            .border_color(theme::rule())
            .child(project)
            .child(stats)
            .child(actions)
            .into_any_element()
    }
}
