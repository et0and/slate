//! Small shared controls. Quiet surfaces, precise rules, room to breathe.

use gpui::{Div, ElementId, SharedString, Stateful, Styled, div, prelude::*, px};

use crate::theme;

pub fn micro(value: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(9.0))
        .text_color(theme::muted())
        .child(value.into())
}

pub fn label(value: impl Into<SharedString>) -> Div {
    div()
        .font_family(theme::FONT_UI)
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_size(px(10.0))
        .text_color(theme::muted())
        .child(value.into())
}

pub fn rule() -> Div {
    div().h(px(1.0)).w_full().bg(theme::rule())
}

pub fn section_header(index: &str, title: &str, meta: impl Into<SharedString>) -> Div {
    div()
        .h(px(40.0))
        .flex_shrink_0()
        .px(px(14.0))
        .flex()
        .items_center()
        .justify_between()
        .border_b_1()
        .border_color(theme::rule())
        .child(
            div()
                .flex()
                .items_center()
                .gap(px(8.0))
                .child(
                    div()
                        .font_family(theme::FONT_MONO)
                        .text_size(px(10.0))
                        .text_color(theme::faint())
                        .child("▾"),
                )
                .child(
                    div()
                        .font_family(theme::FONT_MONO)
                        .text_size(px(9.0))
                        .text_color(theme::faint())
                        .child(index.to_string()),
                )
                .child(
                    div()
                        .font_family(theme::FONT_UI)
                        .text_size(px(11.5))
                        .font_weight(gpui::FontWeight::SEMIBOLD)
                        .child(title.to_string()),
                ),
        )
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(9.0))
                .text_color(theme::faint())
                .child(meta.into()),
        )
}

pub fn metric(name: &str, value: impl Into<SharedString>) -> Div {
    div()
        .flex()
        .flex_col()
        .gap(px(2.0))
        .child(
            div()
                .font_family(theme::FONT_UI)
                .text_size(px(9.0))
                .text_color(theme::faint())
                .child(name.to_string()),
        )
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(11.0))
                .text_color(theme::ink())
                .child(value.into()),
        )
}

pub fn button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(11.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .border_1()
        .border_color(theme::rule())
        .bg(theme::surface())
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_size(px(10.5))
        .text_color(theme::ink())
        .hover(|style| style.bg(theme::accent_soft()).border_color(theme::faint()))
        .child(text.into())
}

pub fn primary_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(6.0))
        .border_1()
        .border_color(theme::ink())
        .bg(theme::ink())
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_size(px(10.5))
        .text_color(theme::paper())
        .hover(|style| style.opacity(0.84))
        .child(text.into())
}

pub fn mini_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(22.0))
        .w(px(22.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .text_size(px(12.0))
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
        .child(text.into())
}

pub fn text_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(22.0))
        .px(px(6.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .text_size(px(10.0))
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
        .child(text.into())
}

pub fn toggle(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .w(px(34.0))
        .h(px(20.0))
        .p(px(2.0))
        .flex()
        .items_center()
        .cursor_pointer()
        .bg(if on { theme::ink() } else { theme::rule() })
        .child(
            div()
                .w(px(16.0))
                .h(px(16.0))
                .ml(px(if on { 14.0 } else { 0.0 }))
                .bg(if on { theme::paper() } else { theme::faint() }),
        )
}

pub fn shortcut_label(key: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("⌘{key}")
    }
    #[cfg(not(target_os = "macos"))]
    {
        format!("Ctrl+{key}")
    }
}

pub fn command_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    hint: String,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(10.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .border_1()
        .border_color(theme::rule())
        .bg(theme::surface())
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .font_weight(gpui::FontWeight::MEDIUM)
        .text_size(px(10.0))
        .text_color(theme::ink())
        .hover(|style| style.bg(theme::accent_soft()).border_color(theme::faint()))
        .child(text.into())
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(8.5))
                .text_color(theme::faint())
                .child(hint),
        )
}

pub fn primary_command_button(
    id: impl Into<ElementId>,
    text: impl Into<SharedString>,
    hint: String,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(11.0))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(8.0))
        .border_1()
        .border_color(theme::ink())
        .bg(theme::ink())
        .cursor_pointer()
        .font_family(theme::FONT_MONO)
        .font_weight(gpui::FontWeight::SEMIBOLD)
        .text_size(px(10.0))
        .text_color(theme::paper())
        .hover(|style| style.opacity(0.84))
        .child(text.into())
        .child(
            div()
                .font_family(theme::FONT_MONO)
                .text_size(px(8.5))
                .text_color(theme::paper())
                .opacity(0.7)
                .child(hint),
        )
}
