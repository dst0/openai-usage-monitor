use serde::{Deserialize, Serialize};

/// A display or window rectangle in global AppKit coordinates.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct WindowRect {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WindowRect {
    pub fn new(x: f64, y: f64, width: f64, height: f64) -> Result<Self, String> {
        let rect = Self {
            x,
            y,
            width,
            height,
        };
        if !rect.is_valid() {
            return Err("Window or screen rectangle is invalid".into());
        }
        Ok(rect)
    }

    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width >= 1.0
            && self.height >= 1.0
    }

    pub fn contains(self, other: Self) -> bool {
        other.x >= self.x
            && other.y >= self.y
            && other.x + other.width <= self.x + self.width
            && other.y + other.height <= self.y + self.height
    }
}
