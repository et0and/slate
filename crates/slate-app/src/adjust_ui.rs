//! Inspector adjustments: the Tone, Colour and Effects sections, slider
//! drags, live re-rendering and undo.

use std::sync::Arc;

use gpui::{
    AnyElement, Context, ElementId, MouseButton, MouseDownEvent, Pixels, SharedString, canvas, div,
    prelude::*, px, relative,
};
use slate_core::adjust::{Adjustments, POSTERIZE_DEFAULT, POSTERIZE_MAX, POSTERIZE_MIN, Slider};
use slate_core::model::{PhotoId, QuarterTurns};

use crate::app::{ContactSheet, PanelSection, render_image, rotate_render_image};
use crate::assets;
use crate::history::PhotoFieldChange;
use crate::sidebar::panel_section;
use crate::theme;
use crate::widgets::{
    button, checkbox, disabled_button, icon, mono_text, stepper, text_button, ui_text,
};

const TONE: [Slider; 5] = [
    Slider::Exposure,
    Slider::Brightness,
    Slider::Contrast,
    Slider::Shadows,
    Slider::Highlights,
];
const COLOUR: [Slider; 5] = [
    Slider::Temperature,
    Slider::Tint,
    Slider::Vibrance,
    Slider::Saturation,
    Slider::Hue,
];

/// A slider being dragged. The live value goes straight into the collection;
/// one undo step is recorded on release.
#[derive(Debug, Clone, Copy)]
pub struct AdjustDrag {
    slider: Slider,
    photo: PhotoId,
    before: Adjustments,
}

/// At most one inspector re-render runs at a time. A change while one runs
/// queues exactly one more, so a fast drag never piles up work.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum InspectorRender {
    Idle,
    Busy,
    Stale,
}

fn changed_sliders(adjustments: &Adjustments, sliders: &[Slider]) -> usize {
    sliders
        .iter()
        .filter(|slider| adjustments.slider(**slider) != slider.neutral())
        .count()
}

fn section_changes(section: PanelSection, adjustments: &Adjustments) -> usize {
    match section {
        PanelSection::Tone => changed_sliders(adjustments, &TONE),
        PanelSection::Colour => {
            changed_sliders(adjustments, &COLOUR) + usize::from(adjustments.monochrome)
        }
        PanelSection::Effects => {
            usize::from(adjustments.invert)
                + usize::from(adjustments.posterize.is_some())
                + usize::from(adjustments.threshold.is_some())
        }
        _ => 0,
    }
}

fn reset_section(section: PanelSection, adjustments: &mut Adjustments) {
    match section {
        PanelSection::Tone => {
            for slider in TONE {
                adjustments.set_slider(slider, slider.neutral());
            }
        }
        PanelSection::Colour => {
            for slider in COLOUR {
                adjustments.set_slider(slider, slider.neutral());
            }
            adjustments.monochrome = false;
        }
        PanelSection::Effects => {
            adjustments.invert = false;
            adjustments.posterize = None;
            adjustments.threshold = None;
        }
        _ => {}
    }
}

impl ContactSheet {
    /// Re-render the inspected photo from its cached decode with the current
    /// adjustments and rotation.
    pub(crate) fn render_inspector_image(&mut self, cx: &mut Context<Self>) {
        if self.inspector_render != InspectorRender::Idle {
            self.inspector_render = InspectorRender::Stale;
            return;
        }
        let Some((photo_id, base)) = self.inspector_base.clone() else {
            return;
        };
        let Some(turns) = self
            .project
            .photo(photo_id)
            .map(slate_core::Photo::rotation)
        else {
            return;
        };
        let adjustments = self.project.adjustments.for_photo(photo_id);
        let generation = self.generation;
        self.inspector_render = InspectorRender::Busy;
        cx.spawn(async move |this, cx| {
            let image = cx
                .background_spawn(async move {
                    let mut preview = (*base).clone();
                    preview.adjust(&adjustments);
                    let image = render_image(preview);
                    if turns == QuarterTurns::NONE {
                        image
                    } else {
                        rotate_render_image(&image, turns).unwrap_or(image)
                    }
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                let again = this.inspector_render == InspectorRender::Stale;
                this.inspector_render = InspectorRender::Idle;
                if this.generation == generation && this.inspected_photo == Some(photo_id) {
                    this.inspector_image = Some((photo_id, image));
                    this.inspector_image_error = None;
                    cx.notify();
                }
                if again {
                    this.render_inspector_image(cx);
                }
            });
        })
        .detach();
    }

    /// Decode a fresh thumbnail for `id` without dropping the current one, so
    /// the grid never flashes a placeholder after an adjustment.
    pub(crate) fn refresh_thumb(&mut self, id: PhotoId, cx: &mut Context<Self>) {
        let Some(path) = self.project.photo(id).map(|photo| photo.path.to_path_buf()) else {
            return;
        };
        if self.thumbs_queued.insert(id) {
            self.thumb_queue.push_front((id, path));
        }
        self.pump_thumbs(cx);
    }

    /// Bring every view of `ids` up to date with their stored adjustments.
    /// Only cached thumbnails are refreshed; the rest decode adjusted when
    /// they scroll into view.
    pub(crate) fn adjustments_changed(&mut self, ids: &[PhotoId], cx: &mut Context<Self>) {
        if ids.is_empty() {
            return;
        }
        for id in ids {
            if self.thumbs.contains_key(id) {
                self.refresh_thumb(*id, cx);
            }
        }
        if self.inspected_photo.is_some_and(|id| ids.contains(&id)) {
            self.render_inspector_image(cx);
        }
        if self.photo_preview_window.is_some()
            && let Some(target) = self.photo_preview_target
            && ids.contains(&target)
        {
            self.load_photo_preview(target, cx);
        }
        cx.notify();
    }

    fn commit_adjustments(&mut self, id: PhotoId, before: Adjustments, cx: &mut Context<Self>) {
        let after = self.project.adjustments.for_photo(id);
        if after == before {
            return;
        }
        let before_dirty = self.dirty;
        self.dirty = true;
        self.push_photo_edit(
            vec![PhotoFieldChange::Adjustments { id, before, after }],
            Arc::clone(&self.selected),
            before_dirty,
        );
        self.adjustments_changed(&[id], cx);
    }

    fn edit_adjustments(
        &mut self,
        id: PhotoId,
        edit: impl FnOnce(&mut Adjustments),
        cx: &mut Context<Self>,
    ) {
        let before = self.project.adjustments.for_photo(id);
        let mut after = before;
        edit(&mut after);
        self.project.adjustments.set(id, after);
        self.commit_adjustments(id, before, cx);
    }

    fn begin_slider_drag(
        &mut self,
        slider: Slider,
        event: &MouseDownEvent,
        cx: &mut Context<Self>,
    ) {
        let Some(photo) = self.inspected_photo else {
            return;
        };
        if event.click_count >= 2 {
            self.edit_adjustments(
                photo,
                |adjustments| {
                    adjustments.set_slider(slider, slider.neutral());
                },
                cx,
            );
            return;
        }
        let before = self.project.adjustments.for_photo(photo);
        self.adjust_drag = Some(AdjustDrag {
            slider,
            photo,
            before,
        });
        self.drag_slider(event.position.x, cx);
    }

    pub(crate) fn drag_slider(&mut self, x: Pixels, cx: &mut Context<Self>) {
        let Some(drag) = self.adjust_drag else {
            return;
        };
        let Some(bounds) = self
            .slider_tracks
            .get(&drag.slider)
            .map(|track| track.get())
        else {
            return;
        };
        let width = f32::from(bounds.size.width);
        if width <= 0.0 {
            return;
        }
        let at = ((f32::from(x) - f32::from(bounds.left())) / width).clamp(0.0, 1.0);
        let range = drag.slider.range();
        let current = self.project.adjustments.for_photo(drag.photo);
        let mut next = current;
        next.set_slider(drag.slider, at.mul_add(range.max - range.min, range.min));
        if next != current {
            self.project.adjustments.set(drag.photo, next);
            self.render_inspector_image(cx);
            cx.notify();
        }
    }

    pub(crate) fn end_slider_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.adjust_drag.take() {
            self.commit_adjustments(drag.photo, drag.before, cx);
            cx.notify();
        }
    }

    pub(crate) fn copy_adjustments_to_selection(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.inspected_photo else {
            return;
        };
        let adjustments = self.project.adjustments.for_photo(source);
        let mut fields = Vec::new();
        let mut ids = Vec::new();
        for id in self.selected.iter().filter(|id| *id != source) {
            let before = self.project.adjustments.for_photo(id);
            if before != adjustments {
                fields.push(PhotoFieldChange::Adjustments {
                    id,
                    before,
                    after: adjustments,
                });
                ids.push(id);
            }
        }
        for id in &ids {
            self.project.adjustments.set(*id, adjustments);
        }
        if !ids.is_empty() {
            let before_dirty = self.dirty;
            self.dirty = true;
            self.push_photo_edit(fields, Arc::clone(&self.selected), before_dirty);
            self.log(format!("Adjustments copied / {} photo(s)", ids.len()));
        }
        self.set_status(format!("Adjustments copied to {} photo(s)", ids.len()));
        self.adjustments_changed(&ids, cx);
        cx.notify();
    }

    /// The Tone, Colour and Effects sections for the inspected photo, then
    /// the actions that reach beyond it.
    pub(crate) fn render_adjustments(
        &mut self,
        photo: PhotoId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let adjustments = self.project.adjustments.for_photo(photo);
        let tone_rows: Vec<AnyElement> = TONE
            .into_iter()
            .map(|slider| self.slider_row(slider, &adjustments, cx))
            .collect();
        let mut colour_rows: Vec<AnyElement> = COLOUR
            .into_iter()
            .map(|slider| self.slider_row(slider, &adjustments, cx))
            .collect();
        colour_rows.push(toggle_row(
            "adjust-monochrome",
            "Black & white",
            adjustments.monochrome,
            Some(
                text_button("adjust-desaturate", "Desaturate")
                    .on_click(cx.listener(move |this, _, _, cx| {
                        this.edit_adjustments(
                            photo,
                            |adjustments| {
                                adjustments
                                    .set_slider(Slider::Saturation, Slider::Saturation.range().min);
                            },
                            cx,
                        );
                    }))
                    .into_any_element(),
            ),
            cx.listener(move |this, _, _, cx| {
                this.edit_adjustments(
                    photo,
                    |adjustments| {
                        adjustments.monochrome = !adjustments.monochrome;
                    },
                    cx,
                );
            }),
        ));

        let levels = adjustments.posterize;
        let mut effect_rows = vec![
            toggle_row(
                "adjust-invert",
                "Invert",
                adjustments.invert,
                None,
                cx.listener(move |this, _, _, cx| {
                    this.edit_adjustments(
                        photo,
                        |adjustments| {
                            adjustments.invert = !adjustments.invert;
                        },
                        cx,
                    );
                }),
            ),
            toggle_row(
                "adjust-posterize",
                "Posterize",
                levels.is_some(),
                levels.map(|levels| {
                    stepper(
                        "posterize-fewer",
                        "posterize-more",
                        format!("{levels}"),
                        cx.listener(move |this, _, _, cx| {
                            this.edit_adjustments(
                                photo,
                                |adjustments| {
                                    adjustments.posterize =
                                        Some(levels.saturating_sub(1).max(POSTERIZE_MIN));
                                },
                                cx,
                            );
                        }),
                        cx.listener(move |this, _, _, cx| {
                            this.edit_adjustments(
                                photo,
                                |adjustments| {
                                    adjustments.posterize = Some((levels + 1).min(POSTERIZE_MAX));
                                },
                                cx,
                            );
                        }),
                    )
                }),
                cx.listener(move |this, _, _, cx| {
                    this.edit_adjustments(
                        photo,
                        |adjustments| {
                            adjustments.posterize = match adjustments.posterize {
                                Some(_) => None,
                                None => Some(POSTERIZE_DEFAULT),
                            };
                        },
                        cx,
                    );
                }),
            ),
            toggle_row(
                "adjust-threshold",
                "Threshold",
                adjustments.threshold.is_some(),
                None,
                cx.listener(move |this, _, _, cx| {
                    this.edit_adjustments(
                        photo,
                        |adjustments| {
                            adjustments.threshold = match adjustments.threshold {
                                Some(_) => None,
                                None => Some(Slider::Threshold.neutral() as u8),
                            };
                        },
                        cx,
                    );
                }),
            ),
        ];
        if adjustments.threshold.is_some() {
            effect_rows.push(self.slider_row(Slider::Threshold, &adjustments, cx));
        }

        let sections = [
            (PanelSection::Tone, "Tone", tone_rows),
            (PanelSection::Colour, "Colour", colour_rows),
            (PanelSection::Effects, "Effects", effect_rows),
        ];
        let mut blocks = Vec::new();
        for (section, title, rows) in sections {
            let changes = section_changes(section, &adjustments);
            let reset = (changes > 0).then(|| {
                div()
                    .id(ElementId::Name(SharedString::from(format!(
                        "reset-{}",
                        section.key()
                    ))))
                    .size(px(20.0))
                    .my(px(-3.0))
                    .flex()
                    .items_center()
                    .justify_center()
                    .cursor_pointer()
                    .hover(|style| style.bg(theme::accent_soft()))
                    .on_click(cx.listener(move |this, _, _, cx| {
                        cx.stop_propagation();
                        this.edit_adjustments(
                            photo,
                            |adjustments| {
                                reset_section(section, adjustments);
                            },
                            cx,
                        );
                    }))
                    .child(icon(assets::UNDO, 12.0, theme::muted()))
                    .into_any_element()
            });
            let open = !self.collapsed_sections.contains(&section);
            blocks.push(panel_section(
                section,
                title,
                if changes > 0 {
                    format!("{changes} changed")
                } else {
                    String::new()
                },
                open,
                div().pb(px(8.0)).children(rows).into_any_element(),
                reset,
                cx,
            ));
        }

        let selected_others = self.selected.len() - usize::from(self.selected.contains(photo));
        let copy: AnyElement = if selected_others == 0 || adjustments.is_neutral() {
            disabled_button("Copy to selected")
                .flex_1()
                .into_any_element()
        } else {
            button("adjust-copy", format!("Copy to {selected_others} selected"))
                .flex_1()
                .on_click(cx.listener(|this, _, _, cx| this.copy_adjustments_to_selection(cx)))
                .into_any_element()
        };
        let reset: AnyElement = if adjustments.is_neutral() {
            disabled_button("Reset").into_any_element()
        } else {
            button("adjust-reset", "Reset")
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.edit_adjustments(
                        photo,
                        |adjustments| {
                            *adjustments = Adjustments::default();
                        },
                        cx,
                    );
                }))
                .into_any_element()
        };

        div()
            .flex()
            .flex_col()
            .children(blocks)
            .child(
                div()
                    .border_t_1()
                    .border_color(theme::rule())
                    .px(px(16.0))
                    .py(px(12.0))
                    .flex()
                    .gap(px(6.0))
                    .child(copy)
                    .child(reset),
            )
            .into_any_element()
    }

    fn slider_row(
        &mut self,
        slider: Slider,
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let value = adjustments.slider(slider);
        let range = slider.range();
        let span = range.max - range.min;
        let at = ((value - range.min) / span).clamp(0.0, 1.0);
        let origin = ((slider.neutral() - range.min) / span).clamp(0.0, 1.0);
        let changed = value != slider.neutral();
        let track = self.slider_tracks.entry(slider).or_default().clone();
        let id = format!("slider-{slider:?}").to_lowercase();
        let selector = id.clone();
        div()
            .px(px(16.0))
            .py(px(5.0))
            .flex()
            .flex_col()
            .gap(px(4.0))
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .child(
                        ui_text(
                            theme::TEXT_BODY,
                            if changed {
                                theme::ink()
                            } else {
                                theme::muted()
                            },
                        )
                        .child(slider.label()),
                    )
                    .child(
                        mono_text(
                            theme::TEXT_META,
                            if changed {
                                theme::ink()
                            } else {
                                theme::faint()
                            },
                        )
                        .child(slider.format(value)),
                    ),
            )
            .child(
                div()
                    .id(ElementId::Name(SharedString::from(id)))
                    .debug_selector(move || selector)
                    .relative()
                    .h(px(14.0))
                    .w_full()
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.begin_slider_drag(slider, event, cx);
                        }),
                    )
                    // Record where the track was laid out, so a drag maps the
                    // pointer to a value without guessing the panel geometry.
                    .child(
                        canvas(move |bounds, _, _| track.set(bounds), |_, (), _, _| {})
                            .absolute()
                            .top(px(0.0))
                            .left(px(0.0))
                            .size_full(),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(6.5))
                            .left(px(0.0))
                            .right(px(0.0))
                            .h(px(1.0))
                            .bg(theme::rule()),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(6.0))
                            .left(relative(at.min(origin)))
                            .w(relative((at - origin).abs()))
                            .h(px(2.0))
                            .bg(theme::ink()),
                    )
                    .child(
                        div()
                            .absolute()
                            .top(px(2.0))
                            .left(relative(at))
                            .ml(px(-5.0))
                            .size(px(10.0))
                            .bg(if changed {
                                theme::ink()
                            } else {
                                theme::surface()
                            })
                            .border_1()
                            .border_color(if changed {
                                theme::ink()
                            } else {
                                theme::muted()
                            }),
                    ),
            )
            .into_any_element()
    }
}

fn toggle_row(
    id: &'static str,
    label: &'static str,
    on: bool,
    extra: Option<AnyElement>,
    on_toggle: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .h(px(34.0))
        .px(px(16.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .child(
            ui_text(
                theme::TEXT_BODY,
                if on { theme::ink() } else { theme::muted() },
            )
            .child(label),
        )
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .children(extra)
                .child(checkbox(id, on).on_click(on_toggle)),
        )
        .into_any_element()
}
