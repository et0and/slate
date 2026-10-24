//! Shared controls and typed text styles.

use gpui::{
    AnyElement, Div, ElementId, FontWeight, SharedString, Stateful, Styled, div, prelude::*, px,
    svg,
};

use crate::{assets, theme};

pub fn ui_text(size: f32, color: gpui::Rgba) -> Div {
    div()
        .font_family(theme::FONT_UI)
        .text_size(px(size))
        .text_color(color)
}

pub fn mono_text(size: f32, color: gpui::Rgba) -> Div {
    div()
        .font_family(theme::FONT_MONO)
        .text_size(px(size))
        .text_color(color)
}

pub fn icon(path: &'static str, size: f32, color: gpui::Rgba) -> gpui::Svg {
    svg()
        .path(path)
        .size(px(size))
        .flex_shrink_0()
        .text_color(color)
}

pub fn micro(value: impl Into<SharedString>) -> Div {
    mono_text(theme::TEXT_META, theme::muted()).child(value.into())
}

pub fn label(value: impl Into<SharedString>) -> Div {
    ui_text(theme::TEXT_LABEL, theme::muted())
        .font_weight(FontWeight::MEDIUM)
        .child(value.into())
}

pub fn rule() -> Div {
    div().h(px(1.0)).w_full().bg(theme::rule())
}

pub fn group_label(text: &str) -> Div {
    ui_text(theme::TEXT_LABEL, theme::muted())
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

pub fn setting_row(text: &str, control: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .min_h(px(theme::ROW_H + 2.0))
        .px(px(20.0))
        .border_b_1()
        .border_color(theme::rule())
        .child(ui_text(theme::TEXT_BODY - 0.5, theme::muted()).child(text.to_string()))
        .child(control)
        .into_any_element()
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ButtonKind {
    Outline,
    Primary,
}

fn button_frame(id: impl Into<ElementId>, kind: ButtonKind) -> Stateful<Div> {
    let primary = kind == ButtonKind::Primary;
    let frame = div()
        .id(id)
        .h(px(theme::CONTROL_H))
        .px(px(if primary { 13.0 } else { 12.0 }))
        .flex()
        .items_center()
        .justify_center()
        .gap(px(7.0))
        .border_1()
        .border_color(if primary {
            theme::accent()
        } else {
            theme::rule()
        })
        .bg(if primary {
            theme::accent()
        } else {
            theme::surface()
        })
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .font_weight(if primary {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::MEDIUM
        })
        .text_size(px(theme::TEXT_BODY))
        .text_color(if primary {
            theme::accent_text()
        } else {
            theme::ink()
        })
        .active(|style| style.opacity(0.82));
    if primary {
        frame.shadow_sm().hover(|style| style.opacity(0.88))
    } else {
        frame.hover(|style| style.bg(theme::accent_soft()))
    }
}

pub fn button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    button_frame(id, ButtonKind::Outline).child(text.into())
}

pub fn button_with_icon(
    id: impl Into<ElementId>,
    icon_path: &'static str,
    text: impl Into<SharedString>,
) -> Stateful<Div> {
    button_frame(id, ButtonKind::Outline)
        .child(icon(icon_path, 14.0, theme::muted()))
        .child(text.into())
}

pub fn primary_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    button_frame(id, ButtonKind::Primary).child(text.into())
}

pub fn disabled_button(text: impl Into<SharedString>) -> Div {
    div()
        .h(px(theme::CONTROL_H))
        .px(px(12.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme::rule())
        .font_family(theme::FONT_UI)
        .font_weight(FontWeight::MEDIUM)
        .text_size(px(theme::TEXT_BODY))
        .text_color(theme::faint())
        .child(text.into())
}

pub fn icon_button(id: impl Into<ElementId>, icon_path: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(28.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
        .active(|style| style.opacity(0.72))
        .child(icon(icon_path, 14.0, theme::muted()))
}

pub fn mini_button(id: impl Into<ElementId>, icon_path: &'static str) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(24.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
        .active(|style| style.opacity(0.72))
        .child(icon(icon_path, 12.0, theme::muted()))
}

pub fn text_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(24.0))
        .px(px(8.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .font_family(theme::FONT_UI)
        .font_weight(FontWeight::MEDIUM)
        .text_size(px(theme::TEXT_LABEL))
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
        .active(|style| style.opacity(0.72))
        .child(text.into())
}

pub fn checkbox(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(20.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(theme::rule())
        .cursor_pointer()
        .bg(if on {
            theme::accent()
        } else {
            theme::surface()
        })
        .active(|style| style.opacity(0.72))
        .children(on.then(|| icon(assets::CHECK, 11.0, theme::accent_text())))
}

pub fn stepper(
    minus_id: &'static str,
    plus_id: &'static str,
    value: String,
    on_minus: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
    on_plus: impl Fn(&gpui::ClickEvent, &mut gpui::Window, &mut gpui::App) + 'static,
) -> AnyElement {
    div()
        .flex()
        .items_center()
        .gap(px(2.0))
        .child(mini_button(minus_id, assets::MINUS).on_click(on_minus))
        .child(
            mono_text(theme::TEXT_BODY, theme::ink())
                .w(px(48.0))
                .text_align(gpui::TextAlign::Center)
                .child(value),
        )
        .child(mini_button(plus_id, assets::PLUS).on_click(on_plus))
        .into_any_element()
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

pub fn shift_shortcut_label(key: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("⇧⌘{key}")
    }
    #[cfg(not(target_os = "macos"))]
    {
        format!("Ctrl+Shift+{key}")
    }
}
