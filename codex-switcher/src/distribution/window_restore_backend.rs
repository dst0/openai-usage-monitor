use super::window_restore_capture::WindowCapture;
use super::window_restore_process_identity::ProcessIdentity;

pub trait WindowRestoreBackend {
    fn inspect_process(&mut self, expected_pid: u32) -> Result<ProcessIdentity, String>;
    fn capture_main_window(&mut self, process: ProcessIdentity) -> Result<WindowCapture, String>;
    fn set_position(
        &mut self,
        process: ProcessIdentity,
        position: (f64, f64),
    ) -> Result<(), String>;
    fn set_size(&mut self, process: ProcessIdentity, size: (f64, f64)) -> Result<(), String>;
    fn read_main_window(&mut self, process: ProcessIdentity) -> Result<WindowCapture, String>;
}
