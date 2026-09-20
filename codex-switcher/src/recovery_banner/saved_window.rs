use super::window_rect::WindowRect;
use serde::{Deserialize, Serialize};

/// The saved Codex window and the exact display coordinate space it occupied.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct SavedWindow {
    pub frame: WindowRect,
    pub screen: WindowRect,
}

impl SavedWindow {
    pub fn new(frame: WindowRect, screen: WindowRect) -> Result<Self, String> {
        if !frame.is_valid() || !screen.is_valid() {
            return Err("Saved Codex window geometry is invalid".into());
        }
        Ok(Self { frame, screen })
    }
}
