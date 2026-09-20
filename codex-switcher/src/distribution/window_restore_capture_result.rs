use super::window_restore_capture::WindowCapture;
use super::window_restore_report::RestoreReport;

#[derive(Clone, Debug, PartialEq)]
pub struct WindowCaptureResult {
    pub capture: Option<WindowCapture>,
    pub report: RestoreReport,
}
