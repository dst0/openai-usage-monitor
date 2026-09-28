use super::window_restore_frame::WindowFrame;

/// One ChatGPT window captured before an explicitly requested restart: its
/// WindowServer ID, Accessibility frame (top-left origin), selected task, and
/// whether it had focus. `Debug` never shows the task ID, so an error or log
/// line that formats an entry cannot leak it.
#[derive(Clone, PartialEq)]
pub struct WindowTaskEntry {
    pub window_id: u32,
    pub frame: WindowFrame,
    pub task_id: String,
    pub focused: bool,
}

impl std::fmt::Debug for WindowTaskEntry {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WindowTaskEntry")
            .field("window_id", &self.window_id)
            .field("frame", &self.frame)
            .field("task_id", &"<task>")
            .field("focused", &self.focused)
            .finish()
    }
}
