//! Right rail: live page preview, sheet settings and saved sheets.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px, rgba};

use sheet_core::layout;

use crate::app::ContactSheet;
use crate::theme;
use crate::widgets::{
    button, label, micro, mini_button, primary_button, rule, section_header, text_button, toggle,
};

const PREVIEW_WIDTH: f32 = 300.0;

impl ContactSheet {
    pub(crate) fn render_inspector(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page_count = self.page_count();
        let current = self.current_page();
        let photos = self.export_photos();
        let sheet_layout = layout::SheetLayout::from_settings(&self.project.sheet);
        let page = layout::plan_page(&self.project.sheet, &sheet_layout, &photos, current);
        let scale = PREVIEW_WIDTH / sheet_layout.page_w_mm;
        let preview_height = sheet_layout.page_h_mm * scale;
        let empty = photos.is_empty();

        let mut preview_children: Vec<AnyElement> = Vec::new();

        if !empty {
            if self.project.sheet.header {
                let rule_y = (sheet_layout.content_top_mm - 3.0) * scale;
                preview_children.push(
                    div()
                        .absolute()
                        .top(px(rule_y))
                        .left(px(sheet_layout.margin_mm * scale))
                        .right(px(sheet_layout.margin_mm * scale))
                        .h(px(0.6))
                        .bg(theme::ink())
                        .into_any_element(),
                );
                preview_children.push(
                    div()
                        .absolute()
                        .top(px(sheet_layout.margin_mm * scale))
                        .left(px(sheet_layout.margin_mm * scale))
                        .font_family(theme::FONT_MONO)
                        .text_size(px(4.5))
                        .text_color(theme::ink())
                        .child(
                            self.project
                                .sheet
                                .title_or(&self.project.name)
                                .to_uppercase(),
                        )
                        .into_any_element(),
                );
            }

            {
                for placement in &page.placements {
                    let photo = photos.get(placement.photo_index);
                    let image =
                        photo.and_then(|photo| self.preview_image(photo.id, placement.rotated));
                    let cell_x = placement.cell_x_mm * scale;
                    let cell_y = placement.cell_y_mm * scale;
                    let cell_w = sheet_layout.cell_w_mm * scale;
                    let cell_h = sheet_layout.cell_h_mm * scale;
                    let image_x = (placement.image_x_mm - placement.cell_x_mm) * scale;
                    let image_y = (placement.image_y_mm - placement.cell_y_mm) * scale;
                    let image_w = placement.image_w_mm * scale;
                    let image_h = placement.image_h_mm * scale;
                    let caption_h = sheet_layout.caption_mm * scale;

                    let image_element: AnyElement = match image {
                        Some(image) => img(image)
                            .object_fit(gpui::ObjectFit::Contain)
                            .w(px(image_w))
                            .h(px(image_h))
                            .into_any_element(),
                        None => div()
                            .w(px(image_w))
                            .h(px(image_h))
                            .bg(theme::photo_bg())
                            .into_any_element(),
                    };

                    preview_children.push(
                        div()
                            .absolute()
                            .top(px(cell_y))
                            .left(px(cell_x))
                            .w(px(cell_w))
                            .h(px(cell_h))
                            .border_1()
                            .border_color(rgba(0x11111022))
                            .child(
                                div()
                                    .absolute()
                                    .top(px(image_y))
                                    .left(px(image_x))
                                    .child(image_element),
                            )
                            .children(placement.caption.as_ref().map(|caption| {
                                div()
                                    .absolute()
                                    .top(px(sheet_layout.image_h_mm * scale + 1.0))
                                    .left(px(1.0))
                                    .w(px((cell_w - 2.0).max(1.0)))
                                    .h(px((caption_h - 1.0).max(1.0)))
                                    .overflow_hidden()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(4.2))
                                    .text_color(theme::muted())
                                    .child(caption.clone())
                            }))
                            .into_any_element(),
                    );
                }
            }

            if self.project.sheet.footer {
                let rule_y = (sheet_layout.page_h_mm - sheet_layout.margin_mm - 7.0) * scale;
                preview_children.push(
                    div()
                        .absolute()
                        .top(px(rule_y))
                        .left(px(sheet_layout.margin_mm * scale))
                        .right(px(sheet_layout.margin_mm * scale))
                        .h(px(0.5))
                        .bg(theme::rule())
                        .into_any_element(),
                );
                preview_children.push(
                    div()
                        .absolute()
                        .top(px(rule_y + 2.0))
                        .left(px(sheet_layout.margin_mm * scale))
                        .font_family(theme::FONT_MONO)
                        .text_size(px(4.2))
                        .text_color(theme::faint())
                        .child(format!("PAGE {:02} / {:02}", current + 1, page_count))
                        .into_any_element(),
                );
            }
        }

        if empty {
            preview_children.push(
                div()
                    .absolute()
                    .top(px(preview_height / 2.0 - 8.0))
                    .w_full()
                    .flex()
                    .justify_center()
                    .font_family(theme::FONT_MONO)
                    .text_size(px(6.0))
                    .text_color(theme::faint())
                    .child("NO PHOTOS SELECTED")
                    .into_any_element(),
            );
        }

        let preview = div()
            .w(px(PREVIEW_WIDTH))
            .h(px(preview_height))
            .relative()
            .bg(theme::white())
            .border_1()
            .border_color(theme::ink())
            .children(preview_children);

        let preview_block = div()
            .p(px(10.0))
            .flex()
            .flex_col()
            .gap(px(6.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(label("PAGE PREVIEW"))
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap(px(4.0))
                            .child(mini_button("page-prev", "‹").on_click(cx.listener(
                                |this, _, _, cx| {
                                    let page = this.current_page().saturating_sub(1);
                                    this.goto_page(page, cx);
                                },
                            )))
                            .child(
                                div()
                                    .font_family(theme::FONT_MONO)
                                    .text_size(px(8.0))
                                    .text_color(theme::muted())
                                    .child(format!("PAGE {:02} / {:02}", current + 1, page_count)),
                            )
                            .child(mini_button("page-next", "›").on_click(cx.listener(
                                |this, _, _, cx| {
                                    let page = (this.current_page() + 1)
                                        .min(this.page_count().saturating_sub(1));
                                    this.goto_page(page, cx);
                                },
                            ))),
                    ),
            )
            .child(preview);

        let paper_value = format!(
            "{} / {}",
            self.project.sheet.paper.label(),
            self.project.sheet.orientation.label()
        );
        let columns = self.project.sheet.columns;
        let rows = self.project.sheet.rows;
        let margin = self.project.sheet.margin_mm;
        let gutter = self.project.sheet.gutter_mm;

        let grid_control = div()
            .flex()
            .items_center()
            .gap(px(10.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        mini_button("cols-minus", "−")
                            .on_click(cx.listener(|this, _, _, cx| this.nudge_columns(-1, cx))),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .child(format!("{columns}C")),
                    )
                    .child(
                        mini_button("cols-plus", "+")
                            .on_click(cx.listener(|this, _, _, cx| this.nudge_columns(1, cx))),
                    ),
            )
            .child(
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        mini_button("rows-minus", "−")
                            .on_click(cx.listener(|this, _, _, cx| this.nudge_rows(-1, cx))),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .child(format!("{rows}R")),
                    )
                    .child(
                        mini_button("rows-plus", "+")
                            .on_click(cx.listener(|this, _, _, cx| this.nudge_rows(1, cx))),
                    ),
            );

        let margin_control = stepper(
            "margin-minus",
            "margin-plus",
            format!("{margin:.0} MM"),
            cx.listener(|this, _, _, cx| this.nudge_margin(-2.0, cx)),
            cx.listener(|this, _, _, cx| this.nudge_margin(2.0, cx)),
        );

        let gutter_control = stepper(
            "gutter-minus",
            "gutter-plus",
            format!("{gutter:.0} MM"),
            cx.listener(|this, _, _, cx| this.nudge_gutter(-1.0, cx)),
            cx.listener(|this, _, _, cx| this.nudge_gutter(1.0, cx)),
        );

        let settings = div()
            .flex()
            .flex_col()
            .px(px(10.0))
            .py(px(8.0))
            .child(label("SHEET TITLE"))
            .child(
                div()
                    .mt(px(3.0))
                    .mb(px(4.0))
                    .child(self.title_field.clone()),
            )
            .child(setting_row(
                "PAPER / ORIENTATION",
                div()
                    .flex()
                    .items_center()
                    .gap(px(5.0))
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .child(paper_value),
                    )
                    .child(
                        mini_button("paper-next", "›")
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_paper(cx))),
                    ),
            ))
            .child(setting_row(
                "ORIENTATION",
                button("orientation-toggle", self.project.sheet.orientation.label())
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_orientation(cx))),
            ))
            .child(setting_row("GRID / COLUMNS × ROWS", grid_control))
            .child(setting_row("MARGIN", margin_control))
            .child(setting_row("GUTTER", gutter_control))
            .child(setting_row(
                "CAPTIONS / FILENAMES",
                toggle("captions-toggle", self.project.sheet.captions)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_captions(cx))),
            ))
            .child(setting_row(
                "INDEX NUMBERS",
                toggle("index-toggle", self.project.sheet.index)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_index(cx))),
            ))
            .child(setting_row(
                "ROTATE TO FIT",
                toggle("rotate-toggle", self.project.sheet.rotate)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_rotate(cx))),
            ))
            .child(setting_row(
                "HEADER BAND",
                toggle("header-toggle", self.project.sheet.header)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_header(cx))),
            ))
            .child(setting_row(
                "FOOTER BAND",
                toggle("footer-toggle", self.project.sheet.footer)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_footer(cx))),
            ))
            .child(
                div()
                    .py(px(7.0))
                    .font_family(theme::FONT_MONO)
                    .text_size(px(8.0))
                    .text_color(theme::faint())
                    .child(self.summary()),
            );

        let export_button: AnyElement = if self.exporting.is_some() {
            primary_button("export-pdf", "EXPORTING…").into_any_element()
        } else if empty {
            div()
                .flex_1()
                .h(px(30.0))
                .flex()
                .items_center()
                .justify_center()
                .border_1()
                .border_color(theme::rule())
                .font_family(theme::FONT_MONO)
                .text_size(px(9.0))
                .text_color(theme::faint())
                .child("EXPORT PDF")
                .into_any_element()
        } else {
            primary_button("export-pdf", "EXPORT PDF")
                .flex_1()
                .on_click(cx.listener(|this, _, _, cx| this.export_current(cx)))
                .into_any_element()
        };

        let action_row = div()
            .px(px(10.0))
            .py(px(8.0))
            .flex()
            .gap(px(4.0))
            .border_t_1()
            .border_color(theme::rule())
            .child(
                button("save-sheet", "SAVE SHEET")
                    .flex_1()
                    .on_click(cx.listener(|this, _, _, cx| this.save_sheet(cx))),
            )
            .child(export_button);

        let saved_sheets = self.project.saved_sheets.clone();
        let saved_count = saved_sheets.len();
        let mut saved_rows: Vec<AnyElement> = Vec::new();
        for sheet in saved_sheets {
            let id = sheet.id;
            let name = sheet.name.clone();
            let settings = sheet.settings.clone();
            let count = sheet.photo_ids.len();
            let meta = format!(
                "{} / {} × {} / {} PHOTO(S)",
                settings.paper.label(),
                settings.columns,
                settings.rows,
                count
            );
            let load_id = sheet.id;
            saved_rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(2.0))
                    .px(px(10.0))
                    .py(px(6.0))
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(9.0))
                            .font_weight(gpui::FontWeight::BOLD)
                            .truncate()
                            .child(name),
                    )
                    .child(
                        div()
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::faint())
                            .child(meta),
                    )
                    .child(
                        div()
                            .mt(px(2.0))
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                text_button(
                                    ElementId::Name(SharedString::from(format!(
                                        "load-sheet-{}",
                                        load_id.0
                                    ))),
                                    "LOAD",
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.load_sheet(id, cx)),
                                ),
                            )
                            .child(
                                text_button(
                                    ElementId::Name(SharedString::from(format!(
                                        "export-sheet-{}",
                                        load_id.0
                                    ))),
                                    "EXPORT PDF",
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.export_saved(id, cx)),
                                ),
                            )
                            .child(
                                text_button(
                                    ElementId::Name(SharedString::from(format!(
                                        "delete-sheet-{}",
                                        load_id.0
                                    ))),
                                    "DEL",
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.delete_sheet(id, cx)),
                                ),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let saved_block =
            div()
                .flex()
                .flex_col()
                .child(
                    div()
                        .px(px(10.0))
                        .py(px(6.0))
                        .flex()
                        .items_center()
                        .justify_between()
                        .border_t_1()
                        .border_color(theme::rule())
                        .child(label("SAVED SHEETS"))
                        .child(micro(format!("{saved_count:02}"))),
                )
                .child(div().flex().flex_col().children(saved_rows).children(
                    if saved_count == 0 {
                        Some(
                            div()
                                .px(px(10.0))
                                .py(px(8.0))
                                .font_family(theme::FONT_MONO)
                                .text_size(px(8.0))
                                .text_color(theme::faint())
                                .child("NO SAVED SHEETS / SAVE A LAYOUT TO REUSE IT"),
                        )
                    } else {
                        None
                    },
                ));

        div()
            .w(px(340.0))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .border_l_1()
            .border_color(theme::rule())
            .child(section_header(
                "03",
                "SHEET",
                format!("{:04}", photos.len()),
            ))
            .child(
                div()
                    .id("inspector-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(preview_block)
                    .child(rule())
                    .child(settings)
                    .child(action_row)
                    .child(saved_block)
                    .child(
                        div()
                            .px(px(10.0))
                            .py(px(8.0))
                            .font_family(theme::FONT_MONO)
                            .text_size(px(8.0))
                            .text_color(theme::faint())
                            .child("FLAT PDF / RGB / 300 DPI IMAGE CEILING / NO LAYERS"),
                    ),
            )
            .into_any_element()
    }
}

fn setting_row(text: &str, control: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .min_h(px(30.0))
        .border_b_1()
        .border_color(theme::rule())
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(8.0))
                .text_color(theme::faint())
                .child(text.to_string()),
        )
        .child(control)
        .into_any_element()
}

fn stepper(
    minus_id: &'static str,
    plus_id: &'static str,
    value: String,
    on_minus: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
    on_plus: impl Fn(&gpui::ClickEvent, &mut Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .child(mini_button(minus_id, "−").on_click(on_minus))
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(8.0))
                .w(px(38.0))
                .text_align(gpui::TextAlign::Center)
                .child(value),
        )
        .child(mini_button(plus_id, "+").on_click(on_plus))
        .into_any_element()
}
