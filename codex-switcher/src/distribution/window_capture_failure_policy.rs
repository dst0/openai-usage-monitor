use super::window_capture_mode::WindowCaptureMode;
use super::window_restore_report::RestoreReport;

pub(super) fn classify_capture_failure(
    report: &RestoreReport,
) -> Result<WindowCaptureMode, String> {
    let event = report.events.last();
    if event.is_some_and(|event| {
        event.phase == "CAPTURE_WINDOW_FAILED"
            && event.detail == "Main window capture failed: WINDOW_NOT_FOUND"
    }) {
        return Ok(WindowCaptureMode::Absent);
    }
    Err(event
        .map(|event| event.detail.clone())
        .unwrap_or_else(|| "Codex window capture did not complete successfully".into()))
}

pub(crate) fn optional_banner_capture_failure(error: &str) -> bool {
    matches!(
        error,
        "WINDOW_NOT_FOUND" | "WINDOW_ACCESS_FAILED" | "WINDOW_GEOMETRY_FAILED"
    )
}
