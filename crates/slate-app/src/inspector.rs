//! Right rail: live page preview, sheet settings and saved sheets.

use gpui::{AnyElement, Context, ElementId, SharedString, Window, div, img, prelude::*, px};

use slate_core::layout;
use slate_core::model::{Paper, StarRating};

use crate::app::{ContactSheet, InspectorMode, PhotoMetaState};
use crate::assets;
use crate::theme;
use crate::widgets::{
    button, button_with_icon, checkbox, group_label, icon, icon_button, label, micro, mini_button,
    mono_text, primary_button, setting_row, stepper, ui_text,
};

const PREVIEW_WIDTH: f32 = 300.0;

impl ContactSheet {
    pub(crate) fn render_sheet_preview(
        &mut self,
        width: f32,
        current: usize,
        interactive: bool,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let page_count = self.page_count();
        let current = current.min(page_count.saturating_sub(1));
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
        let scale = width / sheet_layout.page_w_mm;
        let preview_height = sheet_layout.page_h_mm * scale;
        let type_scale = (width / PREVIEW_WIDTH).clamp(1.0, 3.0);
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
                        .text_size(px(4.5 * type_scale))
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
                        .overflow_hidden()
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
                                .text_size(px(4.2 * type_scale))
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
                        .text_size(px(4.2 * type_scale))
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
            .w(px(width))
            .h(px(preview_height))
            .relative()
            .overflow_hidden()
            .bg(theme::sheet())
            .border_1()
            .border_color(theme::rule())
            .shadow_lg()
            .children(preview_children);
        if interactive {
            preview
                .id("preview-open")
                .cursor_pointer()
                .hover(|style| style.border_color(theme::accent()))
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_preview_window(window, cx);
                }))
                .into_any_element()
        } else {
            preview.into_any_element()
        }
    }

    pub(crate) fn render_inspector(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        match self.inspector_mode {
            InspectorMode::Photo => self.render_photo_inspector(window, cx),
            InspectorMode::Sheet => self.render_sheet_inspector(window, cx),
        }
    }

    fn render_photo_inspector(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let Some(photo_id) = self.inspected_photo else {
            return self.render_photo_inspector_empty(cx);
        };
        let Some(photo) = self.project.photo(photo_id).cloned() else {
            self.inspected_photo = None;
            return self.render_photo_inspector_empty(cx);
        };
        self.schedule_thumb_ids([photo_id], cx);
        let image = self.preview_image(photo.id(), photo.rotation(), false);
        let selected = self.selected.len();
        let dimensions = photo
            .oriented_size()
            .map(|(width, height)| format!("{width} × {height}"))
            .unwrap_or_else(|| "—".to_string());
        let folder = photo
            .path
            .parent()
            .map(|parent| parent.display().to_string())
            .unwrap_or_else(|| "—".to_string());
        let tags = if photo.tags.is_empty() {
            "No tags".to_string()
        } else {
            photo
                .tags
                .iter()
                .map(|tag| format!("#{tag}"))
                .collect::<Vec<_>>()
                .join("  ")
        };
        let metadata = match self.photo_meta.get(&photo_id) {
            Some(PhotoMetaState::Loading) => None,
            Some(PhotoMetaState::Ready(meta)) => Some(meta.clone()),
            None => None,
        };
        let metadata_loading = matches!(
            self.photo_meta.get(&photo_id),
            Some(PhotoMetaState::Loading)
        );
        let date_error = self.photo_date_error.clone();
        let rating = photo.rating();
        let label = photo.label();

        let thumbnail: AnyElement = match image {
            Some(image) => img(image)
                .object_fit(gpui::ObjectFit::Contain)
                .size_full()
                .into_any_element(),
            None => div()
                .size_full()
                .flex()
                .items_center()
                .justify_center()
                .child(icon(assets::IMAGE, 28.0, theme::faint()))
                .into_any_element(),
        };

        let mut exif_rows: Vec<AnyElement> = Vec::new();
        match metadata {
            Some(meta) if meta.rows.is_empty() => exif_rows.push(
                ui_text(theme::TEXT_META, theme::faint())
                    .child("No EXIF metadata found.")
                    .into_any_element(),
            ),
            Some(meta) => {
                for (label, value) in &meta.rows {
                    exif_rows.push(photo_info_row(label, value));
                }
            }
            None => exif_rows.push(
                ui_text(theme::TEXT_META, theme::faint())
                    .child(if metadata_loading {
                        "Reading EXIF…"
                    } else {
                        "No EXIF metadata found."
                    })
                    .into_any_element(),
            ),
        }

        let stars = div()
            .flex()
            .items_center()
            .gap(px(2.0))
            .children((1..=StarRating::MAX).map(|value| {
                icon(
                    if rating.get() >= value {
                        assets::STAR_FILLED
                    } else {
                        assets::STAR
                    },
                    12.0,
                    if rating.get() >= value {
                        theme::ink()
                    } else {
                        theme::faint()
                    },
                )
            }));

        let label_swatch = div()
            .size(px(16.0))
            .border_1()
            .border_color(theme::rule())
            .bg(if label == slate_core::model::ColorLabel::None {
                theme::surface()
            } else {
                theme::label_color(label)
            });

        let sub = if selected > 1 {
            format!("{selected} selected / showing primary")
        } else {
            "Photo inspector".to_string()
        };

        div()
            .debug_selector(|| "photo-inspector".into())
            .w(px(self.panels.right()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(
                div()
                    .flex_shrink_0()
                    .px(px(20.0))
                    .pt(px(13.0))
                    .pb(px(10.0))
                    .flex()
                    .items_start()
                    .justify_between()
                    .gap(px(8.0))
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.0))
                            .flex()
                            .flex_col()
                            .gap(px(3.0))
                            .child(
                                ui_text(theme::TEXT_BODY, theme::ink())
                                    .font_weight(gpui::FontWeight::SEMIBOLD)
                                    .truncate()
                                    .child(photo.file_name()),
                            )
                            .child(mono_text(theme::TEXT_META, theme::faint()).child(sub)),
                    )
                    .child(
                        icon_button("inspector-collapse", assets::CHEVRON_RIGHT).on_click(
                            cx.listener(|this, _, window, cx| this.toggle_right_pane(window, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .id("photo-inspector-scroll")
                    .flex_1()
                    .min_h(px(0.0))
                    .overflow_y_scroll()
                    .child(
                        div()
                            .mx(px(20.0))
                            .h(px(196.0))
                            .overflow_hidden()
                            .bg(theme::photo_bg())
                            .border_1()
                            .border_color(theme::rule())
                            .shadow_sm()
                            .child(thumbnail),
                    )
                    .child(
                        div()
                            .px(px(20.0))
                            .py(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(6.0))
                            .child(photo_info_row("Dimensions", &dimensions))
                            .child(photo_info_row("File size", &format_bytes(photo.bytes)))
                            .child(photo_info_row(
                                "Type",
                                &photo.extension().to_ascii_uppercase(),
                            ))
                            .child(photo_info_row("Folder", &folder))
                            .child(photo_info_row("Tags", &tags)),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(theme::rule())
                            .px(px(20.0))
                            .py(px(10.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(group_label("Rating and label"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(12.0))
                                    .child(stars)
                                    .child(
                                        div()
                                            .flex()
                                            .items_center()
                                            .gap(px(6.0))
                                            .child(label_swatch)
                                            .child(
                                                ui_text(theme::TEXT_META, theme::muted())
                                                    .child(label.label()),
                                            ),
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(theme::rule())
                            .px(px(20.0))
                            .pt(px(10.0))
                            .pb(px(4.0))
                            .child(group_label("Capture date")),
                    )
                    .child(
                        div()
                            .px(px(20.0))
                            .pb(px(12.0))
                            .flex()
                            .flex_col()
                            .gap(px(7.0))
                            .child(self.photo_date_field.clone())
                            .children(date_error.map(|error| {
                                ui_text(theme::TEXT_META, theme::muted())
                                    .line_height(px(16.0))
                                    .child(error)
                            }))
                            .child(
                                div()
                                    .flex()
                                    .justify_between()
                                    .gap(px(8.0))
                                    .child(
                                        button("photo-inspector-file-date", "Use file date")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.use_file_date(cx)
                                            })),
                                    )
                                    .child(
                                        primary_button("photo-inspector-save-date", "Save date")
                                            .on_click(cx.listener(|this, _, _, cx| {
                                                this.save_photo_date(cx)
                                            })),
                                    ),
                            )
                            .child(
                                ui_text(theme::TEXT_MICRO, theme::faint())
                                    .line_height(px(14.0))
                                    .child(
                                        "Stored in the .slate project. Original EXIF is not changed.",
                                    ),
                            ),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(theme::rule())
                            .px(px(20.0))
                            .pt(px(10.0))
                            .pb(px(4.0))
                            .child(group_label("EXIF")),
                    )
                    .child(
                        div()
                            .px(px(20.0))
                            .pb(px(12.0))
                            .flex()
                            .flex_col()
                            .children(exif_rows),
                    )
                    .child(
                        div()
                            .border_t_1()
                            .border_color(theme::rule())
                            .px(px(20.0))
                            .py(px(12.0))
                            .flex()
                            .gap(px(6.0))
                            .child(
                                button_with_icon("photo-inspector-rotate", assets::ROTATE, "Rotate")
                                    .flex_1()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.rotate_photo(photo_id, cx)
                                    })),
                            )
                            .child(
                                button_with_icon("photo-inspector-reveal", assets::FOLDER, "Reveal")
                                    .flex_1()
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.reveal_photo(photo_id, cx)
                                    })),
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_photo_inspector_empty(&mut self, cx: &mut Context<Self>) -> AnyElement {
        div()
            .w(px(self.panels.right()))
            .flex_shrink_0()
            .h_full()
            .flex()
            .flex_col()
            .bg(theme::panel())
            .child(
                div()
                    .px(px(20.0))
                    .pt(px(13.0))
                    .pb(px(10.0))
                    .flex()
                    .justify_between()
                    .child(group_label("Photo inspector"))
                    .child(
                        icon_button("inspector-collapse", assets::CHEVRON_RIGHT).on_click(
                            cx.listener(|this, _, window, cx| this.toggle_right_pane(window, cx)),
                        ),
                    ),
            )
            .child(
                div()
                    .px(px(20.0))
                    .py(px(24.0))
                    .flex()
                    .flex_col()
                    .gap(px(10.0))
                    .child(
                        icon(assets::IMAGE, 22.0, theme::faint()),
                    )
                    .child(
                        ui_text(theme::TEXT_BODY, theme::ink())
                            .font_weight(gpui::FontWeight::MEDIUM)
                            .child("Select a photo"),
                    )
                    .child(
                        ui_text(theme::TEXT_META, theme::muted())
                            .line_height(px(16.0))
                            .child(
                                "Metadata and controls appear here. Use File > Create contact sheet… to build a sheet.",
                            ),
                    ),
            )
            .into_any_element()
    }

    fn render_sheet_inspector(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let scope_count = self.scope_ids().len();
        let current = self.current_page();
        let page_count = self.page_count();
        let preview = self.render_sheet_preview(PREVIEW_WIDTH, current, true, cx);

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
                    .w(px(52.0))
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
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        icon_button("inspector-collapse", assets::CHEVRON_RIGHT).on_click(
                            cx.listener(|this, _, window, cx| this.toggle_right_pane(window, cx)),
                        ),
                    )
                    .child(page_controls),
            )
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

        let paper = self.project.sheet.paper;
        let paper_label = if paper == Paper::Custom {
            format!(
                "{:.0} × {:.0} mm",
                self.project.sheet.custom_paper_width_mm, self.project.sheet.custom_paper_height_mm
            )
        } else {
            paper.label().to_string()
        };
        let paper_control = button("paper-select", paper_label)
            .px(px(9.0))
            .text_size(px(theme::TEXT_LABEL))
            .on_click(cx.listener(|this, _, _, cx| this.toggle_paper_menu(cx)))
            .child(icon(assets::CHEVRON_DOWN, 11.0, theme::muted()));
        let paper_options: AnyElement = div()
            .mx(px(20.0))
            .mb(px(6.0))
            .border_1()
            .border_color(theme::rule())
            .bg(theme::surface())
            .shadow_sm()
            .children(Paper::ALL.into_iter().enumerate().map(|(index, choice)| {
                let active = choice == paper;
                div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "paper-option-{index}"
                    ))))
                    .h(px(32.0))
                    .px(px(10.0))
                    .flex()
                    .items_center()
                    .justify_between()
                    .cursor_pointer()
                    .border_b_1()
                    .border_color(theme::rule())
                    .bg(if active {
                        theme::accent_soft()
                    } else {
                        theme::surface()
                    })
                    .hover(|style| style.bg(theme::accent_soft()))
                    .on_click(cx.listener(move |this, _, _, cx| this.set_paper(choice, cx)))
                    .child(
                        ui_text(
                            theme::TEXT_BODY,
                            if active { theme::ink() } else { theme::muted() },
                        )
                        .child(choice.label()),
                    )
                    .children(active.then(|| icon(assets::CHECK, 13.0, theme::accent())))
                    .into_any_element()
            }))
            .into_any_element();
        let paper_menu = self.paper_menu_open.then_some(paper_options);
        let custom_paper = (paper == Paper::Custom).then(|| {
            div()
                .px(px(20.0))
                .pb(px(10.0))
                .flex()
                .flex_col()
                .gap(px(6.0))
                .child(label("Custom size"))
                .child(
                    div()
                        .flex()
                        .items_end()
                        .gap(px(8.0))
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(label("Width"))
                                .child(self.custom_width_field.clone()),
                        )
                        .child(
                            div()
                                .flex_1()
                                .flex()
                                .flex_col()
                                .gap(px(3.0))
                                .child(label("Height"))
                                .child(self.custom_height_field.clone()),
                        ),
                )
                .child(
                    mono_text(theme::TEXT_MICRO, theme::faint()).child("Millimetres; 50–2000 mm."),
                )
                .into_any_element()
        });

        let settings = div()
            .flex()
            .flex_col()
            .pb(px(12.0))
            .child(group_label("Layout").px(px(20.0)).pt(px(12.0)).pb(px(4.0)))
            .child(setting_row("Paper", paper_control))
            .children(paper_menu)
            .children(custom_paper)
            .child(setting_row(
                "Orientation",
                button("orientation-toggle", self.project.sheet.orientation.label())
                    .h(px(30.0))
                    .px(px(9.0))
                    .text_size(px(theme::TEXT_LABEL))
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
                            .line_height(px(14.0))
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
            .debug_selector(|| "sheet-inspector".into())
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
                    .child(
                        div()
                            .px(px(20.0))
                            .pt(px(13.0))
                            .pb(px(4.0))
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(group_label("Contact sheet"))
                            .child(
                                div()
                                    .flex()
                                    .items_center()
                                    .gap(px(6.0))
                                    .child(
                                        mono_text(theme::TEXT_META, theme::faint())
                                            .child(format!("{scope_count} photo(s) in scope")),
                                    )
                                    .child(
                                        icon_button("inspector-close-sheet", assets::X).on_click(
                                            cx.listener(|this, _, _, cx| {
                                                this.inspector_mode = InspectorMode::Photo;
                                                cx.notify();
                                            }),
                                        ),
                                    ),
                            ),
                    )
                    .child(preview_block)
                    .child(title_block)
                    .child(settings)
                    .child(saved_block),
            )
            .into_any_element()
    }
}

fn photo_info_row(label: &str, value: &str) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .gap(px(10.0))
        .child(
            ui_text(theme::TEXT_META, theme::muted())
                .flex_shrink_0()
                .child(label.to_string()),
        )
        .child(
            mono_text(theme::TEXT_META, theme::ink())
                .flex_1()
                .min_w(px(0.0))
                .truncate()
                .child(value.to_string()),
        )
        .into_any_element()
}

fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "Unknown size".to_string()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}
