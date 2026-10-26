//! Resizable rail geometry.

use gpui::Window;

pub const DIVIDER_W: f32 = 8.0;
pub const RAIL_W: f32 = 24.0;

const LEFT_MIN: f32 = 250.0;
const LEFT_MAX: f32 = 460.0;
const RIGHT_MIN: f32 = 360.0;
const RIGHT_MAX: f32 = 560.0;
const CENTER_MIN: f32 = 420.0;
const GUTTERS: f32 = DIVIDER_W * 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResizeTarget {
    Left,
    Right,
}

pub struct PanelLayout {
    left: f32,
    right: f32,
    left_collapsed: bool,
    right_collapsed: bool,
    resizing: Option<ResizeTarget>,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            left: 272.0,
            right: 376.0,
            left_collapsed: false,
            right_collapsed: false,
            resizing: None,
        }
    }
}

impl PanelLayout {
    pub const fn left(&self) -> f32 {
        if self.left_collapsed { 0.0 } else { self.left }
    }

    pub const fn right(&self) -> f32 {
        if self.right_collapsed {
            0.0
        } else {
            self.right
        }
    }

    pub const fn left_edge(&self) -> f32 {
        if self.left_collapsed {
            0.0
        } else {
            self.left + DIVIDER_W
        }
    }

    pub fn right_edge(&self) -> f32 {
        if self.right_collapsed {
            RAIL_W
        } else {
            self.right + DIVIDER_W
        }
    }

    pub const fn left_collapsed(&self) -> bool {
        self.left_collapsed
    }

    pub const fn right_collapsed(&self) -> bool {
        self.right_collapsed
    }

    pub const fn toggle_left(&mut self) {
        self.left_collapsed = !self.left_collapsed;
    }

    pub const fn toggle_right(&mut self) {
        self.right_collapsed = !self.right_collapsed;
    }

    pub const fn expand_right(&mut self) {
        self.right_collapsed = false;
    }

    pub const fn resizing(&self) -> Option<ResizeTarget> {
        self.resizing
    }

    pub fn reset(&mut self) {
        let defaults = Self::default();
        self.left = defaults.left;
        self.right = defaults.right;
        self.left_collapsed = false;
        self.right_collapsed = false;
    }

    pub const fn begin(&mut self, target: ResizeTarget) {
        self.resizing = Some(target);
    }

    pub const fn end(&mut self) -> bool {
        self.resizing.take().is_some()
    }

    pub fn clamp(&mut self, total: f32) {
        self.left = self.left.clamp(LEFT_MIN, Self::left_max(total, self.right));
        self.right = self
            .right
            .clamp(RIGHT_MIN, Self::right_max(total, self.left));
    }

    pub fn resize(&mut self, target: ResizeTarget, pointer_x: f32, total: f32) {
        match target {
            ResizeTarget::Left => {
                self.left = pointer_x.clamp(LEFT_MIN, Self::left_max(total, self.right));
            }
            ResizeTarget::Right => {
                let width = total - pointer_x;
                self.right = width.clamp(RIGHT_MIN, Self::right_max(total, self.left));
            }
        }
    }

    pub fn clamp_to_window(&mut self, window: &Window) {
        self.clamp(f32::from(window.viewport_size().width));
    }

    fn left_max(total: f32, right: f32) -> f32 {
        (total - right - CENTER_MIN - GUTTERS).clamp(LEFT_MIN, LEFT_MAX)
    }

    fn right_max(total: f32, left: f32) -> f32 {
        (total - left - CENTER_MIN - GUTTERS).clamp(RIGHT_MIN, RIGHT_MAX)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rails_keep_the_centre_usable() {
        let mut panels = PanelLayout {
            left: 900.0,
            right: 900.0,
            left_collapsed: false,
            right_collapsed: false,
            resizing: None,
        };
        panels.clamp(1440.0);
        assert!(panels.left() <= LEFT_MAX);
        assert!(panels.right() <= RIGHT_MAX);
        assert!(1440.0 - panels.left_edge() - panels.right_edge() >= CENTER_MIN);
    }

    #[test]
    fn narrow_windows_fall_back_to_minimums() {
        let mut panels = PanelLayout::default();
        panels.clamp(900.0);
        assert_eq!(panels.left(), LEFT_MIN);
        assert_eq!(panels.right(), RIGHT_MIN);
    }

    #[test]
    fn panes_collapse_to_a_narrow_rail_and_expand_again() {
        let mut panels = PanelLayout::default();
        panels.toggle_left();
        panels.toggle_right();
        assert_eq!(panels.left(), 0.0);
        assert_eq!(panels.right(), 0.0);
        assert_eq!(panels.left_edge(), 0.0);
        assert_eq!(panels.right_edge(), RAIL_W);
        panels.reset();
        assert!(!panels.left_collapsed());
        assert!(!panels.right_collapsed());
        assert!(panels.left() > 0.0);
        assert!(panels.right() > 0.0);
    }

    #[test]
    fn dragging_never_crosses_the_centre() {
        let mut panels = PanelLayout::default();
        panels.resize(ResizeTarget::Left, 1600.0, 1600.0);
        assert_eq!(panels.left(), LEFT_MAX);
        panels.resize(ResizeTarget::Right, 0.0, 1600.0);
        assert_eq!(panels.right(), RIGHT_MAX);
    }
}
