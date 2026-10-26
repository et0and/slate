//! Shared controls and typed text styles.

use gpui::{
    AnyElement, App, Bounds, Div, ElementId, FontWeight, MouseButton, MouseDownEvent, SharedString,
    Stateful, Styled, Window, canvas, div, fill, point, prelude::*, px, rgba, size, svg,
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

pub fn group_label(text: &str) -> Div {
    ui_text(theme::TEXT_LABEL, theme::muted())
        .font_weight(FontWeight::SEMIBOLD)
        .child(text.to_string())
}

/// The single heading of a dialog, native child window or empty state.
pub fn heading(text: impl Into<SharedString>) -> Div {
    ui_text(theme::TEXT_DISPLAY, theme::ink())
        .font_weight(FontWeight::SEMIBOLD)
        .line_height(px(26.0))
        .child(text.into())
}

pub fn dialog_panel(width: f32) -> Div {
    div()
        .w(px(width))
        .flex()
        .flex_col()
        .overflow_hidden()
        .bg(theme::surface())
        .border_1()
        .border_color(theme::rule())
        .shadow_2xl()
}

pub fn dialog_header(
    title: impl Into<SharedString>,
    subtitle: Option<String>,
    close: Stateful<Div>,
) -> Div {
    div()
        .px(px(20.0))
        .py(px(16.0))
        .flex()
        .items_start()
        .justify_between()
        .gap(px(12.0))
        .border_b_1()
        .border_color(theme::rule())
        .child(
            div()
                .flex()
                .flex_col()
                .min_w(px(0.0))
                .gap(px(4.0))
                .child(heading(title))
                .children(subtitle.map(|subtitle| {
                    mono_text(theme::TEXT_META, theme::faint())
                        .truncate()
                        .child(subtitle)
                })),
        )
        .child(close)
}

pub fn dialog_footer() -> Div {
    div()
        .px(px(20.0))
        .py(px(14.0))
        .flex()
        .items_center()
        .justify_between()
        .gap(px(8.0))
        .border_t_1()
        .border_color(theme::rule())
}

/// Dim the window behind an in-window dialog. The panel occludes the
/// backdrop: without that, every press inside the panel also reaches
/// `on_dismiss` and closes the dialog before its control can handle the click.
pub fn modal(
    panel: Div,
    on_dismiss: impl Fn(&MouseDownEvent, &mut Window, &mut App) + 'static,
) -> AnyElement {
    div()
        .absolute()
        .top(px(0.0))
        .left(px(0.0))
        .size_full()
        .flex()
        .items_center()
        .justify_center()
        .child(
            div()
                .absolute()
                .top(px(0.0))
                .left(px(0.0))
                .size_full()
                .bg(rgba(0x00000088))
                .on_mouse_down(MouseButton::Left, on_dismiss),
        )
        .child(panel.occlude())
        .into_any_element()
}

pub fn setting_row(text: &str, control: impl IntoElement) -> AnyElement {
    div()
        .flex()
        .items_center()
        .justify_between()
        .min_h(px(theme::ROW_H + 2.0))
        .px(px(16.0))
        .border_b_1()
        .border_color(theme::rule())
        .child(ui_text(theme::TEXT_BODY, theme::muted()).child(text.to_string()))
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
        .font_weight(FontWeight::MEDIUM)
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
        .child(icon(icon_path, 15.0, theme::muted()))
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
        .size(px(30.0))
        .flex()
        .items_center()
        .justify_center()
        .cursor_pointer()
        .text_color(theme::muted())
        .hover(|style| style.bg(theme::accent_soft()).text_color(theme::ink()))
        .active(|style| style.opacity(0.72))
        .child(icon(icon_path, 15.0, theme::muted()))
}

pub fn mini_button(id: impl Into<ElementId>, icon_path: &'static str) -> Stateful<Div> {
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
        .child(icon(icon_path, 13.0, theme::muted()))
}

pub fn text_button(id: impl Into<ElementId>, text: impl Into<SharedString>) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(28.0))
        .px(px(9.0))
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

/// One choice in a row or grid of mutually exclusive choices.
pub fn segment_button(
    id: impl Into<ElementId>,
    label: impl Into<SharedString>,
    active: bool,
) -> Stateful<Div> {
    div()
        .id(id)
        .h(px(26.0))
        .flex()
        .items_center()
        .justify_center()
        .border_1()
        .border_color(if active { theme::ink() } else { theme::rule() })
        .bg(if active {
            theme::accent_soft()
        } else {
            theme::surface()
        })
        .cursor_pointer()
        .hover(|style| style.bg(theme::accent_soft()))
        .font_family(theme::FONT_UI)
        .font_weight(FontWeight::MEDIUM)
        .text_size(px(theme::TEXT_LABEL))
        .text_color(if active { theme::ink() } else { theme::muted() })
        .child(label.into())
}

pub fn checkbox(id: impl Into<ElementId>, on: bool) -> Stateful<Div> {
    div()
        .id(id)
        .size(px(22.0))
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
        .children(on.then(|| icon(assets::CHECK, 13.0, theme::accent_text())))
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
                .w(px(52.0))
                .text_align(gpui::TextAlign::Center)
                .child(value),
        )
        .child(mini_button(plus_id, assets::PLUS).on_click(on_plus))
        .into_any_element()
}

/// Shortcut hints sit beside body-size labels, so they use body size too:
/// at metadata size the modifier glyphs are unreadable.
pub fn shortcut_hint(shortcut: String) -> Div {
    mono_text(theme::TEXT_BODY, theme::faint()).child(shortcut)
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

pub fn alt_shortcut_label(key: &str) -> String {
    #[cfg(target_os = "macos")]
    {
        format!("⌥⌘{key}")
    }
    #[cfg(not(target_os = "macos"))]
    {
        format!("Ctrl+Alt+{key}")
    }
}

/// The SLATE wordmark on a 29 × 7 cell grid.
const WORDMARK: [&str; 7] = [
    ".###..#.......#...#####.#####",
    "#...#.#......#.#....#...#....",
    "#.....#.....#...#...#...#....",
    ".###..#.....#...#...#...####.",
    "....#.#.....#####...#...#....",
    "#...#.#.....#...#...#...#....",
    ".###..#####.#...#...#...#####",
];
/// The gap between cells as a fraction of a cell.
const WORDMARK_GAP: f32 = 0.25;

/// A wordmark size in whole device pixels. Every cell and every gap is the
/// same number of pixels, so the letters stay even at any size; rounding a
/// fractional grid cell by cell makes some cells merge and others thin out.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct WordmarkGrid {
    /// Cell and gap edges, in device pixels.
    cell: f32,
    gap: f32,
    scale: f32,
}

impl WordmarkGrid {
    const COLUMNS: f32 = WORDMARK[0].len() as f32;
    const ROWS: f32 = WORDMARK.len() as f32;

    fn with_cell(cell: f32, scale: f32) -> Self {
        Self {
            cell,
            gap: (cell * WORDMARK_GAP).round().max(1.0),
            scale,
        }
    }

    fn extent(self, cells: f32) -> f32 {
        (cells - 1.0).mul_add(self.gap, cells * self.cell)
    }

    /// The largest grid whose `cells` span fits `points`.
    fn fit(points: f32, cells: f32, scale: f32) -> Self {
        let pixels = (points * scale).floor();
        let mut cell = (pixels / (cells - 1.0).mul_add(WORDMARK_GAP, cells))
            .floor()
            .max(1.0)
            + 1.0;
        while cell > 1.0 && Self::with_cell(cell, scale).extent(cells) > pixels {
            cell -= 1.0;
        }
        Self::with_cell(cell, scale)
    }

    /// The largest grid no wider than `width` points.
    #[must_use]
    pub fn fit_width(width: f32, scale: f32) -> Self {
        Self::fit(width, Self::COLUMNS, scale)
    }

    /// The largest grid no taller than `height` points.
    #[must_use]
    pub fn fit_height(height: f32, scale: f32) -> Self {
        Self::fit(height, Self::ROWS, scale)
    }

    #[must_use]
    pub fn width(self) -> f32 {
        self.extent(Self::COLUMNS) / self.scale
    }

    #[must_use]
    pub fn height(self) -> f32 {
        self.extent(Self::ROWS) / self.scale
    }

    /// The gap between cells in points, for marks drawn beside the wordmark.
    #[must_use]
    pub fn gap(self) -> f32 {
        self.gap / self.scale
    }
}

/// The SLATE wordmark, painted as squares rather than a bitmap so it stays
/// crisp at any size and takes any theme colour.
pub fn pixel_wordmark(grid: WordmarkGrid, colour: gpui::Rgba) -> AnyElement {
    canvas(
        |_, _, _| {},
        move |bounds, (), window, _| {
            let WordmarkGrid { cell, gap, scale } = grid;
            let pitch = cell + gap;
            // Start on a device pixel so every edge lands on one.
            let left = (f32::from(bounds.left()) * scale).round();
            let top = (f32::from(bounds.top()) * scale).round();
            for (row, line) in WORDMARK.iter().enumerate() {
                for (column, mark) in line.bytes().enumerate() {
                    if mark != b'#' {
                        continue;
                    }
                    window.paint_quad(fill(
                        Bounds::new(
                            point(
                                px((column as f32).mul_add(pitch, left) / scale),
                                px((row as f32).mul_add(pitch, top) / scale),
                            ),
                            size(px(cell / scale), px(cell / scale)),
                        ),
                        colour,
                    ));
                }
            }
        },
    )
    .w(px(grid.width()))
    .h(px(grid.height()))
    .into_any_element()
}

/// File sizes everywhere in the interface: binary units, a dash when the size
/// is unknown.
pub fn format_bytes(bytes: u64) -> String {
    if bytes == 0 {
        "—".to_string()
    } else if bytes >= 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{} KB", (bytes / 1024).max(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wordmark_grid_is_even_and_fits() {
        assert!(WORDMARK.iter().all(|row| row.len() == WORDMARK[0].len()));
        for scale in [1.0, 2.0, 3.0] {
            for width in [60.0, 93.0, 437.0, 1200.0] {
                let grid = WordmarkGrid::fit_width(width, scale);
                assert!(grid.width() <= width + f32::EPSILON, "{width} at {scale}x");
                assert_eq!(grid.cell.fract(), 0.0);
                assert_eq!(grid.gap.fract(), 0.0);
                assert!(grid.gap >= 1.0);
                // The next size up would not fit.
                let larger = WordmarkGrid::with_cell(grid.cell + 1.0, scale);
                assert!(larger.width() > width);
            }
            let grid = WordmarkGrid::fit_height(22.0, scale);
            assert!(grid.height() <= 22.0 + f32::EPSILON);
        }
    }
}
