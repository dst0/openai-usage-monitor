use super::window_restore_backend::WindowRestoreBackend;
use super::window_restore_capture::WindowCapture;
use super::window_restore_outcome::RestoreOutcome;
use super::window_restore_report::RestoreReport;
use super::window_restore_service::WindowRestoreService;

/// Rebinds a saved window frame to the exact process created by a relaunch.
pub struct WindowRelaunchRestoreService;

impl WindowRelaunchRestoreService {
    /// Inspect the replacement PID, then reuse the normal position-size-
    /// position restore and post-write verification path.
    pub fn restore(
        service: &WindowRestoreService,
        backend: &mut dyn WindowRestoreBackend,
        operation_id: &str,
        reason: &str,
        capture: WindowCapture,
        expected_pid: u32,
    ) -> RestoreReport {
        let mut report = RestoreReport::new(operation_id, reason);
        let process = match backend.inspect_process(expected_pid) {
            Ok(process) if process.is_for(expected_pid) => process,
            Ok(_) => {
                return service.failure(
                    report,
                    "RESTORE_RELAUNCH_PROCESS_MISMATCH",
                    RestoreOutcome::Failed,
                    "Replacement PID did not match the expected Desktop process",
                )
            }
            Err(error) => {
                return service.failure(
                    report,
                    "RESTORE_RELAUNCH_PROCESS_FAILED",
                    RestoreOutcome::Failed,
                    &format!("Replacement process inspection failed: {error}"),
                )
            }
        };
        if !capture.is_valid() {
            return service.failure(
                report,
                "RESTORE_RELAUNCH_CAPTURE_INVALID",
                RestoreOutcome::Failed,
                "Saved window frame or screen identity is invalid",
            );
        }
        let replacement_capture = WindowCapture {
            process,
            frame: capture.frame,
            screen: capture.screen,
        };
        report.record(
            "RESTORE_RELAUNCH_PROCESS",
            RestoreOutcome::InProgress,
            "Replacement PID and birth identity verified",
        );
        let restored = service.restore(backend, operation_id, reason, replacement_capture);
        report.events.extend(restored.events);
        report.outcome = restored.outcome;
        report
    }
}
