//! Small shared controls. Flat edges, hairline rules, mono metadata.

use gpui::{Div, ElementId, SharedString, Stateful, Styled, div, prelude::*, px};

use crate::theme;

pub fn micro(value: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::MICRO))
        .text_color(theme::muted())
        .child(value.into())
}

pub fn micro_light(value: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(theme::MICRO))
        .text_color(theme::paper())
        .child(value.into())
}

pub fn label(value: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(8.0))
        .text_color(theme::muted())
        .child(value.into())
}

pub fn rule() -> Div {
    div().h(px(1.0)).w_full().bg(theme::rule())
}

pub fn section_header(index: &str, title: &str, meta: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(24.0))
        .px(px(10.0))
        .bg(theme::ink())
        .text_color(theme::paper())
        .font_family(theme::FONT_MONO)
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .text_size(px(8.0))
                        .text_color(theme::faint())
                        .child(index.to_string()),
                )
                .child(
                    div()
                        .text_size(px(9.0))
                        .font_weight(gpui::FontWeight::BOLD)
                        .child(title.to_uppercase()),
                ),
        )
        .child(
            div()
                .text_size(px(8.0))
                .text_color(theme::faint())
                .child(meta.into()),
        )
}

pub fn button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(24.0))
        .px(px(9.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme::ink())
        .bg(theme::white())
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .text_size(px(9.0))
        .text_color(theme::ink())
        .hover(|style| style.bg(theme::ink()).text_color(theme::paper()))
        .child(text.into())
}

pub fn primary_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme::ink())
        .bg(theme::ink())
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .text_size(px(9.0))
        .text_color(theme::paper())
        .hover(|style| style.bg(theme::accent()).border_color(theme::accent()))
        .child(text.into())
}

pub fn mini_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(18.0))
        .w(px(18.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme::rule())
        .bg(theme::white())
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .text_size(px(10.0))
        .text_color(theme::ink())
        .hover(|style| style.bg(theme::ink()).text_color(theme::paper()))
        .child(text.into())
}

pub fn text_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(18.0))
        .px(px(5.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .text_size(px(8.0))
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::ink()).text_color(theme::paper()))
        .child(text.into())
}

pub fn toggle(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    let base = div()
        .id(id)
        .h(px(20.0))
        .px(px(7.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .text_size(px(8.0));
    if on {
        base.border_color(theme::ink())
            .bg(theme::ink())
            .text_color(theme::paper())
            .child("ON")
    } else {
        base.border_color(theme::rule())
            .bg(theme::white())
            .text_color(theme::faint())
            .child("OFF")
    }
}

pub fn key_value(label_text: &str, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .items_center()
        .justify_between()
        .h(px(24.0))
        .border_b_1()
        .border_color(theme::rule())
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(8.0))
                .text_color(theme::faint())
                .child(label_text.to_uppercase()),
        )
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(9.0))
                .text_color(theme::ink())
                .child(value.into()),
        )
}
