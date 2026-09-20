use super::window_restore_frame::WindowFrame;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_restore_screen::ScreenIdentity;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct WindowCapture {
    pub process: ProcessIdentity,
    pub frame: WindowFrame,
    pub screen: ScreenIdentity,
}

impl WindowCapture {
    pub fn is_valid(&self) -> bool {
        self.process.pid != 0 && self.frame.is_valid() && self.screen.is_valid()
    }
}
