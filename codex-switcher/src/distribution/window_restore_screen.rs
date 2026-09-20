use super::window_restore_frame::WindowFrame;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct ScreenIdentity {
    pub display_id: u32,
    pub frame: WindowFrame,
}

impl ScreenIdentity {
    pub fn is_valid(self) -> bool {
        self.display_id != 0 && self.frame.is_valid()
    }
}
