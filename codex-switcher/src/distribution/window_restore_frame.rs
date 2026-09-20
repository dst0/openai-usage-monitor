use super::window_restore_tolerance::RestoreTolerance;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowFrame {
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl WindowFrame {
    pub fn is_valid(self) -> bool {
        self.x.is_finite()
            && self.y.is_finite()
            && self.width.is_finite()
            && self.height.is_finite()
            && self.width > 0.0
            && self.height > 0.0
    }

    pub fn approximately_matches(self, other: Self, tolerance: RestoreTolerance) -> bool {
        (self.x - other.x).abs() <= tolerance.position
            && (self.y - other.y).abs() <= tolerance.position
            && (self.width - other.width).abs() <= tolerance.size
            && (self.height - other.height).abs() <= tolerance.size
    }
}
