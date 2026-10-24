//! Resizable rail geometry.

use gpui::Window;

pub const DIVIDER_W: f32 = 8.0;

const LEFT_MIN: f32 = 250.0;
const LEFT_MAX: f32 = 460.0;
const RIGHT_MIN: f32 = 360.0;
const RIGHT_MAX: f32 = 560.0;
const CENTER_MIN: f32 = 420.0;
const GUTTERS: f32 = DIVIDER_W * 2.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ResizeTarget {
    Left,
    Right,
}

pub struct PanelLayout {
    left: f32,
    right: f32,
    resizing: Option<ResizeTarget>,
}

impl Default for PanelLayout {
    fn default() -> Self {
        Self {
            left: 272.0,
            right: 376.0,
            resizing: None,
        }
    }
}

impl PanelLayout {
    pub fn left(&self) -> f32 {
        self.left
    }

    pub fn right(&self) -> f32 {
        self.right
    }

    pub fn resizing(&self) -> Option<ResizeTarget> {
        self.resizing
    }

    pub fn reset(&mut self) {
        let defaults = Self::default();
        self.left = defaults.left;
        self.right = defaults.right;
    }

    pub fn begin(&mut self, target: ResizeTarget) {
        self.resizing = Some(target);
    }

    pub fn end(&mut self) -> bool {
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
            resizing: None,
        };
        panels.clamp(1440.0);
        assert!(panels.left() <= LEFT_MAX);
        assert!(panels.right() <= RIGHT_MAX);
        assert!(1440.0 - panels.left() - panels.right() - GUTTERS >= CENTER_MIN);
    }

    #[test]
    fn narrow_windows_fall_back_to_minimums() {
        let mut panels = PanelLayout::default();
        panels.clamp(900.0);
        assert_eq!(panels.left(), LEFT_MIN);
        assert_eq!(panels.right(), RIGHT_MIN);
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
