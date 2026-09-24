use super::window_capture_mode::WindowCaptureMode;

pub trait AppLifecycle: Send + Sync {
    fn is_app_running(&self) -> bool;
    fn stop_app(&self) -> Result<(), String>;
    fn launch_app(&self) -> Result<Vec<u32>, String>;
    fn capture_window_bounds(
        &self,
        operation_id: &str,
        targets: &[String],
        reason: &str,
        preserve_window_bounds: bool,
    ) -> Result<WindowCaptureMode, String>;
    fn restore_window_bounds(
        &self,
        pid: u32,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), String>;
    fn abort_recovery(&self) {}
    fn recover_threads(&self, targets: &[String]) -> Result<(), String>;
    fn verify_desktop_stable(&self, pids: &[u32], require_window: bool) -> Result<(), String>;
    fn notify_distribution_complete(&self);
}
