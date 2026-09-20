use super::window_restore_backend::WindowRestoreBackend;
use super::window_restore_capture::WindowCapture;
use super::window_restore_capture_result::WindowCaptureResult;
use super::window_restore_outcome::RestoreOutcome;
use super::window_restore_report::RestoreReport;
use super::window_restore_sanitizer::sanitize_operation_id;
use super::window_restore_tolerance::RestoreTolerance;

pub struct WindowRestoreService {
    tolerance: RestoreTolerance,
}

impl WindowRestoreService {
    pub fn new(tolerance: RestoreTolerance) -> Result<Self, String> {
        RestoreTolerance::new(tolerance.position, tolerance.size).map(|_| Self { tolerance })
    }

    pub fn capture(
        &self,
        backend: &mut dyn WindowRestoreBackend,
        operation_id: &str,
        reason: &str,
        expected_pid: u32,
    ) -> WindowCaptureResult {
        let operation_id = sanitize_operation_id(operation_id);
        let mut report = RestoreReport::new(&operation_id, reason);
        if expected_pid == 0 {
            return self.capture_failure(report, "CAPTURE_VALIDATE", "Expected PID is zero");
        }

        let process = match backend.inspect_process(expected_pid) {
            Ok(process) if process.is_for(expected_pid) => process,
            Ok(_process) => {
                return self.capture_failure(
                    report,
                    "CAPTURE_PROCESS_MISMATCH",
                    &format!(
                        "Observed process identity does not match expected pid {expected_pid}"
                    ),
                )
            }
            Err(error) => {
                return self.capture_failure(
                    report,
                    "CAPTURE_PROCESS_FAILED",
                    &format!("Process inspection failed: {error}"),
                )
            }
        };
        report.record(
            "CAPTURE_PROCESS",
            RestoreOutcome::InProgress,
            "Exact process PID and birth identity captured",
        );

        let capture = match backend.capture_main_window(process.clone()) {
            Ok(capture) if capture.process == process && capture.is_valid() => capture,
            Ok(capture) if capture.process != process => {
                return self.capture_failure(
                    report,
                    "CAPTURE_WINDOW_MISMATCH",
                    "Main window process identity differs from inspected process",
                )
            }
            Ok(_) => {
                return self.capture_failure(
                    report,
                    "CAPTURE_WINDOW_INVALID",
                    "Main window frame or screen identity is invalid",
                )
            }
            Err(error) => {
                return self.capture_failure(
                    report,
                    "CAPTURE_WINDOW_FAILED",
                    &format!("Main window capture failed: {error}"),
                )
            }
        };
        report.record(
            "CAPTURE_WINDOW",
            RestoreOutcome::InProgress,
            &format!(
                "Captured exact main window on display {}",
                capture.screen.display_id
            ),
        );
        report.outcome = RestoreOutcome::Restored;
        report.record(
            "CAPTURE_COMPLETE",
            RestoreOutcome::Restored,
            "Window frame and screen identity captured",
        );
        WindowCaptureResult {
            capture: Some(capture),
            report,
        }
    }

    pub fn restore(
        &self,
        backend: &mut dyn WindowRestoreBackend,
        operation_id: &str,
        reason: &str,
        capture: WindowCapture,
    ) -> RestoreReport {
        let mut report = RestoreReport::new(operation_id, reason);
        if !capture.is_valid() {
            return self.failure(
                report,
                "RESTORE_VALIDATE",
                RestoreOutcome::Failed,
                "Captured window identity is invalid",
            );
        }

        let process = match backend.inspect_process(capture.process.pid) {
            Ok(process) if process == capture.process => process,
            Ok(_) => {
                return self.failure(
                    report,
                    "RESTORE_PROCESS_MISMATCH",
                    RestoreOutcome::Failed,
                    "PID or birth identity changed; refusing another process",
                )
            }
            Err(error) => {
                return self.failure(
                    report,
                    "RESTORE_PROCESS_FAILED",
                    RestoreOutcome::Failed,
                    &format!("Process inspection failed: {error}"),
                )
            }
        };
        report.record(
            "RESTORE_PROCESS",
            RestoreOutcome::InProgress,
            "Exact PID and birth identity verified",
        );

        if let Err(error) =
            backend.set_position(process.clone(), (capture.frame.x, capture.frame.y))
        {
            return self.failure(
                report,
                "RESTORE_POSITION_INITIAL",
                RestoreOutcome::Failed,
                &format!("Initial position failed: {error}"),
            );
        }
        report.record(
            "RESTORE_POSITION_INITIAL",
            RestoreOutcome::InProgress,
            "Position applied",
        );

        if let Err(error) =
            backend.set_size(process.clone(), (capture.frame.width, capture.frame.height))
        {
            return self.failure(
                report,
                "RESTORE_SIZE",
                RestoreOutcome::Partial,
                &format!("Size failed after position changed: {error}"),
            );
        }
        report.record("RESTORE_SIZE", RestoreOutcome::InProgress, "Size applied");

        if let Err(error) =
            backend.set_position(process.clone(), (capture.frame.x, capture.frame.y))
        {
            return self.failure(
                report,
                "RESTORE_POSITION_FINAL",
                RestoreOutcome::Partial,
                &format!("Final position failed after size changed: {error}"),
            );
        }
        report.record(
            "RESTORE_POSITION_FINAL",
            RestoreOutcome::InProgress,
            "Final position reapplied",
        );

        let actual = match backend.read_main_window(process.clone()) {
            Ok(actual) => actual,
            Err(error) => {
                return self.failure(
                    report,
                    "RESTORE_VERIFY_READ",
                    RestoreOutcome::Partial,
                    &format!("Post-restore window read failed: {error}"),
                );
            }
        };
        if actual.process != process {
            return self.failure(
                report,
                "RESTORE_VERIFY_PROCESS",
                RestoreOutcome::Partial,
                "Post-restore window belongs to a different process identity",
            );
        }
        if actual.screen.display_id != capture.screen.display_id {
            return self.failure(
                report,
                "RESTORE_VERIFY_SCREEN",
                RestoreOutcome::Partial,
                "Post-restore window is on a different display",
            );
        }
        if !actual
            .frame
            .approximately_matches(capture.frame, self.tolerance)
        {
            return self.failure(
                report,
                "RESTORE_VERIFY_BOUNDS",
                RestoreOutcome::Partial,
                "Post-restore bounds exceed configured tolerance",
            );
        }
        report.record(
            "RESTORE_VERIFY",
            RestoreOutcome::InProgress,
            "Actual bounds and process identity verified",
        );
        report.outcome = RestoreOutcome::Restored;
        report.record(
            "RESTORE_COMPLETE",
            RestoreOutcome::Restored,
            "Window restore verified",
        );
        report
    }

    fn capture_failure(
        &self,
        mut report: RestoreReport,
        phase: &str,
        detail: &str,
    ) -> WindowCaptureResult {
        report.outcome = RestoreOutcome::Failed;
        report.record(phase, RestoreOutcome::Failed, detail);
        WindowCaptureResult {
            capture: None,
            report,
        }
    }

    pub(crate) fn failure(
        &self,
        mut report: RestoreReport,
        phase: &str,
        outcome: RestoreOutcome,
        detail: &str,
    ) -> RestoreReport {
        report.outcome = outcome;
        report.record(phase, outcome, detail);
        report
    }
}
