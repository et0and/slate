//! Inspector adjustments: the collapsible adjustment sections, slider and
//! curve drags, live re-rendering and undo.

use std::sync::Arc;

use gpui::{
    AnyElement, Bounds, Context, ElementId, MouseButton, MouseDownEvent, PathBuilder, Pixels,
    Point, Rgba, SharedString, canvas, div, fill, point, prelude::*, px, relative, size,
};
use slate_core::adjust::{
    AdjustmentMap, Adjustments, CURVE_POINTS, Channel, ColourLookup, Curve, FilterKnob,
    GradientKnob, GradientMap, HueRange, LevelsKnob, MatchColour, MatchKnob, POSTERIZE_DEFAULT,
    POSTERIZE_MAX, POSTERIZE_MIN, PhotoFilter, ReplaceColour, ReplaceKnob, Slider, ToneKnob,
    hue_colour, load_lut_file,
};
use slate_core::import;
use slate_core::model::{PhotoId, QuarterTurns};

use crate::app::{
    ContactSheet, FileKinds, PanelSection, Pick, pick_files, render_image, rotate_render_image,
};
use crate::assets;
use crate::history::PhotoFieldChange;
use crate::sidebar::panel_section;
use crate::theme;
use crate::widgets::{
    button, checkbox, disabled_button, icon, mono_text, segment_button, stepper, text_button,
    ui_text,
};

const TONE: [Slider; 5] = [
    Slider::Exposure,
    Slider::Brightness,
    Slider::Contrast,
    Slider::Shadows,
    Slider::Highlights,
];
const LEVELS: [Slider; 5] = [
    Slider::Levels(LevelsKnob::InputBlack),
    Slider::Levels(LevelsKnob::Gamma),
    Slider::Levels(LevelsKnob::InputWhite),
    Slider::Levels(LevelsKnob::OutputBlack),
    Slider::Levels(LevelsKnob::OutputWhite),
];
const COLOUR: [Slider; 5] = [
    Slider::Temperature,
    Slider::Tint,
    Slider::Vibrance,
    Slider::Saturation,
    Slider::Hue,
];
const FILTER: [Slider; 2] = [
    Slider::Filter(FilterKnob::Colour),
    Slider::Filter(FilterKnob::Density),
];
const GRADIENT: [Slider; 5] = [
    Slider::Gradient(GradientKnob::ShadowHue),
    Slider::Gradient(GradientKnob::ShadowSaturation),
    Slider::Gradient(GradientKnob::HighlightHue),
    Slider::Gradient(GradientKnob::HighlightSaturation),
    Slider::Gradient(GradientKnob::Amount),
];
const REPLACE: [Slider; 5] = [
    Slider::Replace(ReplaceKnob::Colour),
    Slider::Replace(ReplaceKnob::Range),
    Slider::Replace(ReplaceKnob::Tone(ToneKnob::Hue)),
    Slider::Replace(ReplaceKnob::Tone(ToneKnob::Saturation)),
    Slider::Replace(ReplaceKnob::Tone(ToneKnob::Lightness)),
];
const MATCH: [Slider; 3] = [
    Slider::Match(MatchKnob::Luminance),
    Slider::Match(MatchKnob::Intensity),
    Slider::Match(MatchKnob::Fade),
];
const TONE_KNOBS: [ToneKnob; 3] = [ToneKnob::Hue, ToneKnob::Saturation, ToneKnob::Lightness];

/// The inspector's adjustment sections, top to bottom.
pub const ADJUSTMENT_SECTIONS: [PanelSection; 12] = [
    PanelSection::Tone,
    PanelSection::Levels,
    PanelSection::Curves,
    PanelSection::Colour,
    PanelSection::PhotoFilter,
    PanelSection::ChannelMixer,
    PanelSection::SelectiveColour,
    PanelSection::ReplaceColour,
    PanelSection::MatchColour,
    PanelSection::GradientMap,
    PanelSection::ColourLookup,
    PanelSection::Effects,
];

/// The less common sections start collapsed so the panel opens short.
pub const COLLAPSED_BY_DEFAULT: [PanelSection; 9] = [
    PanelSection::Levels,
    PanelSection::Curves,
    PanelSection::PhotoFilter,
    PanelSection::ChannelMixer,
    PanelSection::SelectiveColour,
    PanelSection::ReplaceColour,
    PanelSection::MatchColour,
    PanelSection::GradientMap,
    PanelSection::ColourLookup,
];

/// A control being dragged. The live value goes straight into the
/// collection; one undo step is recorded on release.
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

/// Which part of a multi-part control the inspector is showing. View state
/// only; never stored with the photo.
#[derive(Debug, Clone, Copy)]
pub struct AdjustFocus {
    pub mixer_output: Channel,
    pub selective_range: HueRange,
}

impl Default for AdjustFocus {
    fn default() -> Self {
        Self {
            mixer_output: Channel::Red,
            selective_range: HueRange::Reds,
        }
    }
}

const fn section_title(section: PanelSection) -> &'static str {
    match section {
        PanelSection::Tone => "Tone",
        PanelSection::Levels => "Levels",
        PanelSection::Curves => "Curves",
        PanelSection::Colour => "Colour",
        PanelSection::PhotoFilter => "Photo filter",
        PanelSection::ChannelMixer => "Channel mixer",
        PanelSection::SelectiveColour => "Selective colour",
        PanelSection::ReplaceColour => "Replace colour",
        PanelSection::MatchColour => "Match colour",
        PanelSection::GradientMap => "Gradient map",
        PanelSection::ColourLookup => "Colour lookup",
        _ => "Effects",
    }
}

fn changed(adjustments: &Adjustments, sliders: &[Slider]) -> usize {
    sliders
        .iter()
        .filter(|slider| adjustments.slider(**slider) != slider.neutral())
        .count()
}

fn mixer_sliders() -> impl Iterator<Item = Slider> {
    Channel::ALL
        .into_iter()
        .flat_map(|output| Channel::ALL.map(|input| Slider::Mix { output, input }))
}

fn selective_sliders() -> impl Iterator<Item = Slider> {
    HueRange::ALL
        .into_iter()
        .flat_map(|range| TONE_KNOBS.map(|knob| Slider::Selective(range, knob)))
}

/// How many settings in a section differ from the photo as decoded. A
/// switched-on group counts once even at its starting values.
fn section_changes(section: PanelSection, adjustments: &Adjustments) -> usize {
    match section {
        PanelSection::Tone => changed(adjustments, &TONE),
        PanelSection::Levels => changed(adjustments, &LEVELS),
        PanelSection::Curves => usize::from(adjustments.curve != Curve::default()),
        PanelSection::Colour => changed(adjustments, &COLOUR) + usize::from(adjustments.monochrome),
        PanelSection::PhotoFilter => usize::from(adjustments.photo_filter.is_some()),
        PanelSection::ChannelMixer => changed(adjustments, &mixer_sliders().collect::<Vec<_>>()),
        PanelSection::SelectiveColour => {
            changed(adjustments, &selective_sliders().collect::<Vec<_>>())
        }
        PanelSection::ReplaceColour => usize::from(adjustments.replace.is_some()),
        PanelSection::MatchColour => usize::from(adjustments.match_colour.is_some()),
        PanelSection::GradientMap => usize::from(adjustments.gradient_map.is_some()),
        PanelSection::ColourLookup => usize::from(adjustments.lookup.is_some()),
        PanelSection::Effects => {
            usize::from(adjustments.invert)
                + usize::from(adjustments.posterize.is_some())
                + usize::from(adjustments.threshold.is_some())
        }
        _ => 0,
    }
}

fn reset_section(section: PanelSection, adjustments: &mut Adjustments) {
    let defaults = Adjustments::default();
    let reset = |adjustments: &mut Adjustments, sliders: &[Slider]| {
        for slider in sliders {
            adjustments.set_slider(*slider, slider.neutral());
        }
    };
    match section {
        PanelSection::Tone => reset(adjustments, &TONE),
        PanelSection::Levels => adjustments.levels = defaults.levels,
        PanelSection::Curves => adjustments.curve = defaults.curve,
        PanelSection::Colour => {
            reset(adjustments, &COLOUR);
            adjustments.monochrome = false;
        }
        PanelSection::PhotoFilter => adjustments.photo_filter = None,
        PanelSection::ChannelMixer => adjustments.mixer = defaults.mixer,
        PanelSection::SelectiveColour => adjustments.selective = defaults.selective,
        PanelSection::ReplaceColour => adjustments.replace = None,
        PanelSection::MatchColour => adjustments.match_colour = None,
        PanelSection::GradientMap => adjustments.gradient_map = None,
        PanelSection::ColourLookup => adjustments.lookup = None,
        PanelSection::Effects => {
            adjustments.invert = false;
            adjustments.posterize = None;
            adjustments.threshold = None;
        }
        _ => {}
    }
}

/// Switchable groups have an on/off checkbox in their section header.
const fn group_toggle(section: PanelSection, adjustments: &Adjustments) -> Option<bool> {
    match section {
        PanelSection::PhotoFilter => Some(adjustments.photo_filter.is_some()),
        PanelSection::ReplaceColour => Some(adjustments.replace.is_some()),
        PanelSection::GradientMap => Some(adjustments.gradient_map.is_some()),
        _ => None,
    }
}

fn toggle_group(section: PanelSection, adjustments: &mut Adjustments) {
    match section {
        PanelSection::PhotoFilter => {
            adjustments.photo_filter = match adjustments.photo_filter {
                Some(_) => None,
                None => Some(PhotoFilter::default()),
            };
        }
        PanelSection::ReplaceColour => {
            adjustments.replace = match adjustments.replace {
                Some(_) => None,
                None => Some(ReplaceColour::default()),
            };
        }
        PanelSection::GradientMap => {
            adjustments.gradient_map = match adjustments.gradient_map {
                Some(_) => None,
                None => Some(GradientMap::default()),
            };
        }
        _ => {}
    }
}

/// All curve points share the editor's bounds.
const fn track_key(slider: Slider) -> Slider {
    match slider {
        Slider::Curve(_) => Slider::Curve(0),
        other => other,
    }
}

const fn rgba([r, g, b]: [f32; 3]) -> Rgba {
    Rgba { r, g, b, a: 1.0 }
}

const fn with_alpha(colour: Rgba, a: f32) -> Rgba {
    Rgba { a, ..colour }
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

    fn commit_adjustments(&mut self, id: PhotoId, before: &Adjustments, cx: &mut Context<Self>) {
        let Some(after) = self
            .project
            .adjustments
            .shared(id)
            .or_else(|| (!before.is_neutral()).then(Arc::default))
        else {
            return;
        };
        if *after == *before {
            return;
        }
        let before_dirty = self.dirty;
        self.dirty = true;
        self.push_photo_edit(
            vec![PhotoFieldChange::Adjustments {
                id,
                before: Arc::new(*before),
                after,
            }],
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
        self.commit_adjustments(id, &before, cx);
    }

    fn begin_adjust_drag(
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
                |adjustments| match slider {
                    Slider::Curve(_) => adjustments.curve = Curve::default(),
                    _ => adjustments.set_slider(slider, slider.neutral()),
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
        self.drag_slider(event.position, cx);
    }

    /// A press on the curve picks the nearest of its five points.
    fn begin_curve_drag(&mut self, event: &MouseDownEvent, cx: &mut Context<Self>) {
        let Some(bounds) = self.track_bounds(Slider::Curve(0)) else {
            return;
        };
        let width = f32::from(bounds.size.width).max(1.0);
        let input = (f32::from(event.position.x - bounds.left()) / width * 255.0).clamp(0.0, 255.0);
        let nearest = Curve::INPUTS
            .iter()
            .enumerate()
            .min_by(|(_, a), (_, b)| (*a - input).abs().total_cmp(&(*b - input).abs()))
            .map_or(0, |(index, _)| index);
        self.begin_adjust_drag(Slider::Curve(nearest as u8), event, cx);
    }

    fn track_bounds(&self, slider: Slider) -> Option<Bounds<Pixels>> {
        self.slider_tracks
            .get(&track_key(slider))
            .map(|track| track.get())
            .filter(|bounds| f32::from(bounds.size.width) > 0.0)
    }

    pub(crate) fn drag_slider(&mut self, position: Point<Pixels>, cx: &mut Context<Self>) {
        let Some(drag) = self.adjust_drag else {
            return;
        };
        let Some(bounds) = self.track_bounds(drag.slider) else {
            return;
        };
        let range = drag.slider.range();
        let value = if matches!(drag.slider, Slider::Curve(_)) {
            let height = f32::from(bounds.size.height).max(1.0);
            (1.0 - f32::from(position.y - bounds.top()) / height) * 255.0
        } else {
            let width = f32::from(bounds.size.width);
            range.value_at(f32::from(position.x - bounds.left()) / width)
        };
        let current = self.project.adjustments.for_photo(drag.photo);
        let mut next = current;
        next.set_slider(drag.slider, value);
        if next != current {
            self.project.adjustments.set(drag.photo, next);
            self.render_inspector_image(cx);
            cx.notify();
        }
    }

    pub(crate) fn end_slider_drag(&mut self, cx: &mut Context<Self>) {
        if let Some(drag) = self.adjust_drag.take() {
            self.commit_adjustments(drag.photo, &drag.before, cx);
            cx.notify();
        }
    }

    pub(crate) fn copy_adjustments_to_selection(&mut self, cx: &mut Context<Self>) {
        let Some(source) = self.inspected_photo else {
            return;
        };
        let after = self.project.adjustments.shared(source).unwrap_or_default();
        let neutral = Arc::new(Adjustments::default());
        let mut fields = Vec::new();
        let mut ids = Vec::new();
        for id in self.selected.iter().filter(|id| *id != source) {
            let before = self
                .project
                .adjustments
                .shared(id)
                .unwrap_or_else(|| Arc::clone(&neutral));
            if before != after {
                fields.push(PhotoFieldChange::Adjustments {
                    id,
                    before,
                    after: Arc::clone(&after),
                });
                ids.push(id);
            }
        }
        for id in &ids {
            self.project.adjustments.set_shared(*id, Arc::clone(&after));
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

    /// Match the inspected photo's colours to the first other selected photo.
    fn match_selected(&mut self, photo: PhotoId, cx: &mut Context<Self>) {
        let Some(source) = self.selected.iter().find(|id| *id != photo) else {
            return;
        };
        let Some(path) = self
            .project
            .photo(source)
            .map(|photo| photo.path.to_path_buf())
        else {
            return;
        };
        let generation = self.generation;
        self.set_status("Reading the source photo's colours");
        cx.notify();
        cx.spawn(async move |this, cx| {
            let stats = cx
                .background_spawn(async move {
                    import::preview(&path, 512).map(|preview| preview.colour_stats())
                })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match stats {
                    Ok(target) => {
                        this.edit_adjustments(
                            photo,
                            |adjustments| {
                                adjustments.match_colour = Some(MatchColour::new(source, target));
                            },
                            cx,
                        );
                        this.set_status("Colours matched");
                    }
                    Err(error) => this.set_status(format!("Match colour failed / {error}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    fn load_lut(&mut self, photo: PhotoId, cx: &mut Context<Self>) {
        let generation = self.generation;
        cx.spawn(async move |this, cx| {
            let Some(path) = pick_files(FileKinds::LUTS, "Load LUT", Pick::One)
                .await
                .into_iter()
                .next()
            else {
                return;
            };
            let task_path = path.clone();
            let loaded = cx
                .background_spawn(async move { load_lut_file(&task_path) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation {
                    return;
                }
                match loaded {
                    Ok(lut) => {
                        this.project.adjustments.remember_lut(lut, &path);
                        this.edit_adjustments(
                            photo,
                            |adjustments| {
                                adjustments.lookup = Some(ColourLookup { lut, amount: 100.0 });
                            },
                            cx,
                        );
                    }
                    Err(error) => this.set_status(format!("Could not load the LUT / {error:#}")),
                }
                cx.notify();
            });
        })
        .detach();
    }

    /// Every adjustment section for the inspected photo, then the actions
    /// that reach beyond it.
    pub(crate) fn render_adjustments(
        &mut self,
        photo: PhotoId,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let adjustments = self.project.adjustments.for_photo(photo);
        let mut blocks = Vec::new();
        for section in ADJUSTMENT_SECTIONS {
            let open = !self.collapsed_sections.contains(&section);
            let rows = if open {
                self.section_rows(section, photo, &adjustments, cx)
            } else {
                Vec::new()
            };
            let changes = section_changes(section, &adjustments);
            let action = self.section_action(section, photo, &adjustments, changes, cx);
            blocks.push(panel_section(
                section,
                section_title(section),
                if changes > 0 {
                    format!("{changes} changed")
                } else {
                    String::new()
                },
                open,
                div().pb(px(8.0)).children(rows).into_any_element(),
                action,
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
                        |adjustments| *adjustments = Adjustments::default(),
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

    /// A section header's reset icon, and the on/off checkbox of a
    /// switchable group.
    fn section_action(
        &self,
        section: PanelSection,
        photo: PhotoId,
        adjustments: &Adjustments,
        changes: usize,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let reset = (changes > 0).then(|| {
            div()
                .id(ElementId::Name(SharedString::from(format!(
                    "reset-{}",
                    section.key()
                ))))
                .size(px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .cursor_pointer()
                .hover(|style| style.bg(theme::accent_soft()))
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.edit_adjustments(
                        photo,
                        |adjustments| reset_section(section, adjustments),
                        cx,
                    );
                }))
                .child(icon(assets::UNDO, 12.0, theme::muted()))
        });
        let toggle = group_toggle(section, adjustments).map(|on| {
            let selector = format!("toggle-{}", section.key());
            checkbox(ElementId::Name(SharedString::from(selector.clone())), on)
                .size(px(18.0))
                .debug_selector(move || selector)
                .on_click(cx.listener(move |this, _, _, cx| {
                    cx.stop_propagation();
                    this.edit_adjustments(
                        photo,
                        |adjustments| toggle_group(section, adjustments),
                        cx,
                    );
                }))
        });
        if reset.is_none() && toggle.is_none() {
            return None;
        }
        Some(
            div()
                .my(px(-3.0))
                .flex()
                .items_center()
                .gap(px(6.0))
                .children(reset)
                .children(toggle)
                .into_any_element(),
        )
    }

    fn sliders(
        &mut self,
        sliders: &[Slider],
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        sliders
            .iter()
            .map(|slider| self.slider_row(*slider, adjustments, cx))
            .collect()
    }

    fn section_rows(
        &mut self,
        section: PanelSection,
        photo: PhotoId,
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        match section {
            PanelSection::Tone => self.sliders(&TONE, adjustments, cx),
            PanelSection::Levels => {
                let mut rows = vec![self.histogram(photo, 56.0)];
                rows.extend(self.sliders(&LEVELS, adjustments, cx));
                rows
            }
            PanelSection::Curves => vec![self.curve_editor(photo, adjustments, cx)],
            PanelSection::Colour => {
                let mut rows = self.sliders(&COLOUR, adjustments, cx);
                rows.push(toggle_row(
                    "adjust-monochrome",
                    "Black & white",
                    adjustments.monochrome,
                    Some(
                        text_button("adjust-desaturate", "Desaturate")
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.edit_adjustments(
                                    photo,
                                    |adjustments| {
                                        adjustments.set_slider(
                                            Slider::Saturation,
                                            Slider::Saturation.range().min,
                                        );
                                    },
                                    cx,
                                );
                            }))
                            .into_any_element(),
                    ),
                    cx.listener(move |this, _, _, cx| {
                        this.edit_adjustments(
                            photo,
                            |adjustments| adjustments.monochrome = !adjustments.monochrome,
                            cx,
                        );
                    }),
                ));
                rows
            }
            PanelSection::PhotoFilter if adjustments.photo_filter.is_some() => {
                self.sliders(&FILTER, adjustments, cx)
            }
            PanelSection::ChannelMixer => {
                let output = self.adjust_focus.mixer_output;
                let mut rows = vec![segments(
                    Channel::ALL.map(|channel| {
                        segment_button(
                            ElementId::Name(SharedString::from(format!(
                                "mixer-{}",
                                channel.label().to_lowercase()
                            ))),
                            channel.label(),
                            channel == output,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.adjust_focus.mixer_output = channel;
                            cx.notify();
                        }))
                        .into_any_element()
                    }),
                    3,
                )];
                let inputs = Channel::ALL.map(|input| Slider::Mix { output, input });
                rows.extend(self.sliders(&inputs, adjustments, cx));
                rows
            }
            PanelSection::SelectiveColour => {
                let range = self.adjust_focus.selective_range;
                let mut rows = vec![segments(
                    HueRange::ALL.map(|choice| {
                        segment_button(
                            ElementId::Name(SharedString::from(format!(
                                "selective-{}",
                                choice.label().to_lowercase()
                            ))),
                            choice.label(),
                            choice == range,
                        )
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.adjust_focus.selective_range = choice;
                            cx.notify();
                        }))
                        .into_any_element()
                    }),
                    3,
                )];
                let knobs = TONE_KNOBS.map(|knob| Slider::Selective(range, knob));
                rows.extend(self.sliders(&knobs, adjustments, cx));
                rows
            }
            PanelSection::ReplaceColour if adjustments.replace.is_some() => {
                self.sliders(&REPLACE, adjustments, cx)
            }
            PanelSection::MatchColour => self.match_rows(photo, adjustments, cx),
            PanelSection::GradientMap => adjustments.gradient_map.map_or_else(Vec::new, |map| {
                let mut rows = vec![gradient_bar(map)];
                rows.extend(self.sliders(&GRADIENT, adjustments, cx));
                rows
            }),
            PanelSection::ColourLookup => self.lookup_rows(photo, adjustments, cx),
            PanelSection::Effects => self.effect_rows(photo, adjustments, cx),
            _ => Vec::new(),
        }
    }

    fn match_rows(
        &mut self,
        photo: PhotoId,
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let Some(matched) = adjustments.match_colour else {
            let has_source = self.selected.iter().any(|id| id != photo);
            return vec![action_row(if has_source {
                button("match-selected", "Match selected photo")
                    .flex_1()
                    .on_click(cx.listener(move |this, _, _, cx| this.match_selected(photo, cx)))
                    .into_any_element()
            } else {
                disabled_button("Match selected photo")
                    .flex_1()
                    .into_any_element()
            })];
        };
        let source = self
            .project
            .photo(matched.source)
            .map_or_else(|| "Removed photo".to_string(), slate_core::Photo::file_name);
        let mut rows = vec![source_row(source)];
        rows.extend(self.sliders(&MATCH, adjustments, cx));
        rows
    }

    fn lookup_rows(
        &mut self,
        photo: PhotoId,
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let load = button(
            "lookup-load",
            if adjustments.lookup.is_some() {
                "Change LUT"
            } else {
                "Load LUT"
            },
        )
        .flex_1()
        .on_click(cx.listener(move |this, _, _, cx| this.load_lut(photo, cx)))
        .into_any_element();
        let Some(lookup) = adjustments.lookup else {
            return vec![action_row(load)];
        };
        let name = self
            .project
            .adjustments
            .lut_path(lookup.lut)
            .and_then(|path| path.file_name())
            .map_or_else(
                || "LUT".to_string(),
                |name| name.to_string_lossy().into_owned(),
            );
        let name = if AdjustmentMap::lut_loaded(lookup.lut) {
            name
        } else {
            format!("{name} / missing")
        };
        let mut rows = vec![source_row(name)];
        rows.extend(self.sliders(&[Slider::LookupAmount], adjustments, cx));
        rows.push(action_row(load));
        rows
    }

    fn effect_rows(
        &mut self,
        photo: PhotoId,
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> Vec<AnyElement> {
        let levels = adjustments.posterize;
        let mut rows = vec![
            toggle_row(
                "adjust-invert",
                "Invert",
                adjustments.invert,
                None,
                cx.listener(move |this, _, _, cx| {
                    this.edit_adjustments(
                        photo,
                        |adjustments| adjustments.invert = !adjustments.invert,
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
            rows.push(self.slider_row(Slider::Threshold, adjustments, cx));
        }
        rows
    }

    /// The inspected photo's luminance histogram, as decoded.
    fn histogram(&self, photo: PhotoId, height: f32) -> AnyElement {
        let bins = self
            .inspector_histogram
            .as_ref()
            .filter(|(id, _)| *id == photo)
            .map(|(_, bins)| Arc::clone(bins));
        let bar = with_alpha(theme::muted(), 0.45);
        div()
            .px(px(16.0))
            .pt(px(4.0))
            .pb(px(6.0))
            .child(
                canvas(
                    |_, _, _| {},
                    move |bounds, (), window, _| {
                        if let Some(bins) = &bins {
                            paint_histogram(window, bounds, bins, bar);
                        }
                    },
                )
                .w_full()
                .h(px(height)),
            )
            .into_any_element()
    }

    fn curve_editor(
        &mut self,
        photo: PhotoId,
        adjustments: &Adjustments,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let width = (self.panels.right() - 32.0).max(80.0);
        let height = (width * 0.75).min(220.0);
        let track = self
            .slider_tracks
            .entry(Slider::Curve(0))
            .or_default()
            .clone();
        let bins = self
            .inspector_histogram
            .as_ref()
            .filter(|(id, _)| *id == photo)
            .map(|(_, bins)| Arc::clone(bins));
        let curve = adjustments.curve;
        let (ink, rule, bar) = (theme::ink(), theme::rule(), with_alpha(theme::muted(), 0.3));
        div()
            .px(px(16.0))
            .pt(px(4.0))
            .pb(px(8.0))
            .child(
                div()
                    .id("curve-editor")
                    .debug_selector(|| "curve-editor".into())
                    .w(px(width))
                    .h(px(height))
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::rule())
                    .cursor_pointer()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, event: &MouseDownEvent, _, cx| {
                            cx.stop_propagation();
                            this.begin_curve_drag(event, cx);
                        }),
                    )
                    .child(
                        canvas(
                            move |bounds, _, _| track.set(bounds),
                            move |bounds, (), window, _| {
                                paint_curve(
                                    window,
                                    bounds,
                                    &curve,
                                    bins.as_deref(),
                                    ink,
                                    rule,
                                    bar,
                                );
                            },
                        )
                        .size_full(),
                    ),
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
        let at = range.fraction(value);
        let origin = range.fraction(slider.neutral());
        let changed = value != slider.neutral();
        let track = self.slider_tracks.entry(slider).or_default().clone();
        let id = format!("slider-{slider:?}").to_lowercase();
        let selector = id.clone();
        let rail: AnyElement = if slider.is_hue_picker() {
            canvas(
                |_, _, _| {},
                |bounds, (), window, _| {
                    const STEPS: usize = 48;
                    let width = f32::from(bounds.size.width) / STEPS as f32;
                    for step in 0..STEPS {
                        let hue = step as f32 / STEPS as f32 * 360.0;
                        window.paint_quad(fill(
                            Bounds::new(
                                point(bounds.left() + px(step as f32 * width), bounds.top()),
                                size(px(width + 0.5), bounds.size.height),
                            ),
                            rgba(hue_colour(hue, 0.8)),
                        ));
                    }
                },
            )
            .absolute()
            .top(px(5.0))
            .left(px(0.0))
            .right(px(0.0))
            .h(px(4.0))
            .into_any_element()
        } else {
            div()
                .absolute()
                .top(px(6.5))
                .left(px(0.0))
                .right(px(0.0))
                .h(px(1.0))
                .bg(theme::rule())
                .into_any_element()
        };
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
                            this.begin_adjust_drag(slider, event, cx);
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
                    .child(rail)
                    .children((!slider.is_hue_picker()).then(|| {
                        div()
                            .absolute()
                            .top(px(6.0))
                            .left(relative(at.min(origin)))
                            .w(relative((at - origin).abs()))
                            .h(px(2.0))
                            .bg(theme::ink())
                    }))
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
                                theme::surface()
                            } else {
                                theme::muted()
                            }),
                    ),
            )
            .into_any_element()
    }
}

fn paint_histogram(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    bins: &[u32; 256],
    colour: Rgba,
) {
    let peak = bins.iter().copied().max().unwrap_or(0).max(1) as f32;
    let width = f32::from(bounds.size.width) / 256.0;
    let height = f32::from(bounds.size.height);
    for (index, count) in bins.iter().enumerate() {
        if *count == 0 {
            continue;
        }
        // A square-root scale keeps thin tails visible beside a tall peak.
        let bar = (*count as f32 / peak).sqrt() * height;
        window.paint_quad(fill(
            Bounds::new(
                point(
                    bounds.left() + px(index as f32 * width),
                    bounds.bottom() - px(bar),
                ),
                size(px(width.max(1.0)), px(bar)),
            ),
            colour,
        ));
    }
}

fn paint_curve(
    window: &mut gpui::Window,
    bounds: Bounds<Pixels>,
    curve: &Curve,
    bins: Option<&[u32; 256]>,
    ink: Rgba,
    rule: Rgba,
    bar: Rgba,
) {
    let width = f32::from(bounds.size.width);
    let height = f32::from(bounds.size.height);
    let at = |input: f32, output: f32| {
        point(
            bounds.left() + px(input / 255.0 * width),
            bounds.top() + px((1.0 - output / 255.0) * height),
        )
    };
    if let Some(bins) = bins {
        paint_histogram(window, bounds, bins, bar);
    }
    for quarter in 1..4 {
        let offset = quarter as f32 / 4.0;
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.left() + px(offset * width), bounds.top()),
                size(px(1.0), bounds.size.height),
            ),
            rule,
        ));
        window.paint_quad(fill(
            Bounds::new(
                point(bounds.left(), bounds.top() + px(offset * height)),
                size(bounds.size.width, px(1.0)),
            ),
            rule,
        ));
    }
    let mut identity = PathBuilder::stroke(px(1.0));
    identity.move_to(at(0.0, 0.0));
    identity.line_to(at(255.0, 255.0));
    if let Ok(path) = identity.build() {
        window.paint_path(path, rule);
    }
    let mut line = PathBuilder::stroke(px(1.5));
    line.move_to(at(0.0, curve.sample(0.0)));
    for step in 1..=64 {
        let input = step as f32 / 64.0 * 255.0;
        line.line_to(at(input, curve.sample(input)));
    }
    if let Ok(path) = line.build() {
        window.paint_path(path, ink);
    }
    for index in 0..CURVE_POINTS {
        let centre = at(Curve::INPUTS[index], curve.points[index]);
        window.paint_quad(fill(
            Bounds::new(centre - point(px(3.5), px(3.5)), size(px(7.0), px(7.0))),
            ink,
        ));
    }
}

/// The gradient map's colours from shadow to highlight.
fn gradient_bar(map: GradientMap) -> AnyElement {
    div()
        .px(px(16.0))
        .pt(px(4.0))
        .pb(px(4.0))
        .child(
            canvas(
                |_, _, _| {},
                move |bounds, (), window, _| {
                    const STEPS: usize = 48;
                    let width = f32::from(bounds.size.width) / STEPS as f32;
                    for step in 0..STEPS {
                        let y = (step as f32 + 0.5) / STEPS as f32;
                        window.paint_quad(fill(
                            Bounds::new(
                                point(bounds.left() + px(step as f32 * width), bounds.top()),
                                size(px(width + 0.5), bounds.size.height),
                            ),
                            rgba(map.colour_at(y)),
                        ));
                    }
                },
            )
            .w_full()
            .h(px(14.0)),
        )
        .into_any_element()
}

/// Lay choices out in rows of `columns`.
fn segments<const N: usize>(choices: [AnyElement; N], columns: u16) -> AnyElement {
    div()
        .px(px(16.0))
        .pt(px(2.0))
        .pb(px(6.0))
        .grid()
        .grid_cols(columns)
        .gap(px(4.0))
        .children(choices)
        .into_any_element()
}

fn action_row(control: AnyElement) -> AnyElement {
    div()
        .px(px(16.0))
        .py(px(6.0))
        .flex()
        .child(control)
        .into_any_element()
}

/// What a match or lookup draws from.
fn source_row(name: String) -> AnyElement {
    div()
        .px(px(16.0))
        .py(px(6.0))
        .child(
            mono_text(theme::TEXT_META, theme::muted())
                .truncate()
                .child(name),
        )
        .into_any_element()
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
