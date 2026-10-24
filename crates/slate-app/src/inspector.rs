//! Right rail: live page preview, sheet settings and saved sheets.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px};

use slate_core::layout;

use crate::app::ContactSheet;
use crate::assets;
use crate::theme;
use crate::widgets::{
    button, checkbox, group_label, icon, label, micro, mini_button, mono_text, setting_row,
    stepper, ui_text,
};

const PREVIEW_WIDTH: f32 = 280.0;

impl ContactSheet {
    pub(crate) fn render_inspector(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page_count = self.page_count();
        let current = self.preview_page.min(page_count.saturating_sub(1));
        let scope = self.scope_ids();
        let per_page = self.project.sheet.per_page().max(1);
        let first_index = current.saturating_mul(per_page);
        let page_start = first_index.min(scope.len());
        let page_end = page_start.saturating_add(per_page).min(scope.len());
        let page_photos = scope[page_start..page_end]
            .iter()
            .filter_map(|id| self.project.photo(*id).cloned())
            .collect::<Vec<_>>();
        let sheet_layout = layout::SheetLayout::from_settings(&self.project.sheet);
        let page_refs = page_photos.iter().collect::<Vec<_>>();
        let page = layout::plan_page_refs(
            &self.project.sheet,
            &sheet_layout,
            &page_refs,
            first_index,
            current,
        );
        let page_ids = page_photos
            .iter()
            .map(|photo| photo.id())
            .collect::<Vec<_>>();
        self.schedule_thumb_ids(page_ids, cx);
        let scale = PREVIEW_WIDTH / sheet_layout.page_w_mm;
        let preview_height = sheet_layout.page_h_mm * scale;
        let empty = page_photos.is_empty();

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
                        .bg(theme::sheet_rule())
                        .into_any_element(),
                );
                preview_children.push(
                    div()
                        .absolute()
                        .top(px(sheet_layout.margin_mm * scale))
                        .left(px(sheet_layout.margin_mm * scale))
                        .font_family(theme::FONT_MONO)
                        .text_size(px(4.5))
                        .text_color(theme::sheet_ink())
                        .child(self.project.sheet.title_or(&self.project.name).to_string())
                        .into_any_element(),
                );
            }

            for placement in &page.placements {
                let photo = page_photos.get(placement.photo_index);
                let image = photo.and_then(|photo| {
                    self.preview_image(photo.id(), photo.rotation(), placement.rotated)
                });
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
                        .bg(theme::sheet_photo())
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
                        .border_color(theme::sheet_rule())
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
                                .text_color(theme::sheet_muted())
                                .child(caption.clone())
                        }))
                        .into_any_element(),
                );
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
                        .bg(theme::sheet_rule())
                        .into_any_element(),
                );
                preview_children.push(
                    div()
                        .absolute()
                        .top(px(rule_y + 2.0))
                        .left(px(sheet_layout.margin_mm * scale))
                        .font_family(theme::FONT_MONO)
                        .text_size(px(4.2))
                        .text_color(theme::sheet_muted())
                        .child(format!("Page {} of {page_count}", current + 1))
                        .into_any_element(),
                );
            }
        }

        if empty {
            preview_children.push(
                div()
                    .absolute()
                    .top(px(preview_height / 2.0 - 10.0))
                    .w_full()
                    .flex()
                    .justify_center()
                    .child(icon(assets::IMAGE_OFF, 20.0, theme::sheet_muted()))
                    .into_any_element(),
            );
        }

        let preview = div()
            .w(px(PREVIEW_WIDTH))
            .h(px(preview_height))
            .relative()
            .overflow_hidden()
            .bg(theme::sheet())
            .border_1()
            .border_color(theme::rule())
            .shadow_lg()
            .children(preview_children);

        let page_controls = div()
            .flex()
            .items_center()
            .gap(px(4.0))
            .child(
                mini_button("page-prev", assets::CHEVRON_LEFT).on_click(cx.listener(
                    |this, _, _, cx| {
                        let page = this.current_page().saturating_sub(1);
                        this.goto_page(page, cx);
                    },
                )),
            )
            .child(
                mono_text(theme::TEXT_META, theme::faint())
                    .w(px(46.0))
                    .text_align(gpui::TextAlign::Center)
                    .child(format!("{} / {}", current + 1, page_count)),
            )
            .child(
                mini_button("page-next", assets::CHEVRON_RIGHT).on_click(cx.listener(
                    |this, _, _, cx| {
                        let page =
                            (this.current_page() + 1).min(this.page_count().saturating_sub(1));
                        this.goto_page(page, cx);
                    },
                )),
            );

        let preview_block = div()
            .px(px(20.0))
            .pt(px(14.0))
            .pb(px(14.0))
            .flex()
            .flex_col()
            .gap(px(10.0))
            .child(div().flex().justify_end().child(page_controls))
            .child(div().w_full().flex().justify_center().child(preview));

        let title_block = div()
            .px(px(20.0))
            .pb(px(10.0))
            .flex()
            .flex_col()
            .gap(px(5.0))
            .child(label("Title"))
            .child(self.title_field.clone());

        let columns = self.project.sheet.columns;
        let rows = self.project.sheet.rows;
        let margin = self.project.sheet.margin_mm;
        let gutter = self.project.sheet.gutter_mm;

        let grid_control = div()
            .flex()
            .items_center()
            .gap(px(12.0))
            .child(stepper(
                "cols-minus",
                "cols-plus",
                format!("{columns} col"),
                cx.listener(|this, _, _, cx| this.nudge_columns(-1, cx)),
                cx.listener(|this, _, _, cx| this.nudge_columns(1, cx)),
            ))
            .child(stepper(
                "rows-minus",
                "rows-plus",
                format!("{rows} row"),
                cx.listener(|this, _, _, cx| this.nudge_rows(-1, cx)),
                cx.listener(|this, _, _, cx| this.nudge_rows(1, cx)),
            ));

        let margin_control = stepper(
            "margin-minus",
            "margin-plus",
            format!("{margin:.0} mm"),
            cx.listener(|this, _, _, cx| this.nudge_margin(-2.0, cx)),
            cx.listener(|this, _, _, cx| this.nudge_margin(2.0, cx)),
        );

        let gutter_control = stepper(
            "gutter-minus",
            "gutter-plus",
            format!("{gutter:.0} mm"),
            cx.listener(|this, _, _, cx| this.nudge_gutter(-1.0, cx)),
            cx.listener(|this, _, _, cx| this.nudge_gutter(1.0, cx)),
        );

        let settings = div()
            .flex()
            .flex_col()
            .pb(px(12.0))
            .child(group_label("Layout").px(px(20.0)).pt(px(12.0)).pb(px(4.0)))
            .child(setting_row(
                "Paper",
                div()
                    .flex()
                    .items_center()
                    .gap(px(2.0))
                    .child(
                        mono_text(theme::TEXT_BODY, theme::ink())
                            .child(self.project.sheet.paper.label()),
                    )
                    .child(
                        mini_button("paper-next", assets::CHEVRON_RIGHT)
                            .on_click(cx.listener(|this, _, _, cx| this.cycle_paper(cx))),
                    ),
            ))
            .child(setting_row(
                "Orientation",
                button("orientation-toggle", self.project.sheet.orientation.label())
                    .h(px(26.0))
                    .px(px(9.0))
                    .text_size(px(theme::TEXT_META))
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_orientation(cx))),
            ))
            .child(setting_row("Grid", grid_control))
            .child(setting_row("Margin", margin_control))
            .child(setting_row("Gutter", gutter_control))
            .child(group_label("Marks").px(px(20.0)).pt(px(12.0)).pb(px(4.0)))
            .child(setting_row(
                "Captions",
                checkbox("captions-toggle", self.project.sheet.captions)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_captions(cx))),
            ))
            .child(setting_row(
                "Index numbers",
                checkbox("index-toggle", self.project.sheet.index)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_index(cx))),
            ))
            .child(setting_row(
                "Rotate to fit",
                checkbox("rotate-toggle", self.project.sheet.rotate)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_rotate(cx))),
            ))
            .child(setting_row(
                "Header",
                checkbox("header-toggle", self.project.sheet.header)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_header(cx))),
            ))
            .child(setting_row(
                "Footer",
                checkbox("footer-toggle", self.project.sheet.footer)
                    .on_click(cx.listener(|this, _, _, cx| this.toggle_footer(cx))),
            ));

        let saved_sheets = self.project.saved_sheets.clone();
        let saved_count = saved_sheets.len();
        let mut saved_rows: Vec<AnyElement> = Vec::new();
        for sheet in saved_sheets {
            let id = sheet.id;
            let name = sheet.name.clone();
            let settings = sheet.settings.clone();
            let count = sheet.photo_ids.len();
            let photo_label = if count == 1 { "photo" } else { "photos" };
            let meta = format!(
                "{} / {} × {} / {} {photo_label}",
                settings.paper.label(),
                settings.columns,
                settings.rows,
                count
            );
            saved_rows.push(
                div()
                    .flex()
                    .flex_col()
                    .gap(px(5.0))
                    .px(px(20.0))
                    .py(px(10.0))
                    .border_b_1()
                    .border_color(theme::rule())
                    .child(
                        ui_text(theme::TEXT_BODY, theme::ink())
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .truncate()
                            .child(name),
                    )
                    .child(
                        mono_text(theme::TEXT_MICRO, theme::faint())
                            .line_height(px(12.0))
                            .child(meta),
                    )
                    .child(
                        div()
                            .mt(px(1.0))
                            .flex()
                            .items_center()
                            .gap(px(2.0))
                            .child(
                                mini_button(
                                    ElementId::Name(SharedString::from(format!(
                                        "load-sheet-{}",
                                        id.0
                                    ))),
                                    assets::EXTERNAL_LINK,
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.load_sheet(id, cx)),
                                ),
                            )
                            .child(
                                mini_button(
                                    ElementId::Name(SharedString::from(format!(
                                        "export-sheet-{}",
                                        id.0
                                    ))),
                                    assets::DOWNLOAD,
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.export_saved(id, cx)),
                                ),
                            )
                            .child(
                                mini_button(
                                    ElementId::Name(SharedString::from(format!(
                                        "delete-sheet-{}",
                                        id.0
                                    ))),
                                    assets::TRASH,
                                )
                                .on_click(
                                    cx.listener(move |this, _, _, cx| this.delete_sheet(id, cx)),
                                ),
                            ),
                    )
                    .into_any_element(),
            );
        }

        let saved_block = div()
            .flex()
            .flex_col()
            .child(
                div()
                    .px(px(20.0))
                    .pt(px(14.0))
                    .pb(px(8.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .border_t_1()
                    .border_color(theme::rule())
                    .child(group_label("Saved sheets"))
                    .child(micro(format!("{saved_count}"))),
            )
            .children(saved_rows)
            .children(if saved_count == 0 {
                Some(
                    ui_text(theme::TEXT_META, theme::faint())
                        .px(px(20.0))
                        .pb(px(14.0))
                        .line_height(px(14.0))
                        .child("No saved sheets. Save a layout to reuse it."),
                )
            } else {
                None
            });

        div()
            .w(px(self.panels.right()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(
                div()
                    .id("inspector-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(preview_block)
                    .child(title_block)
                    .child(settings)
                    .child(saved_block),
            )
            .into_any_element()
    }
}
