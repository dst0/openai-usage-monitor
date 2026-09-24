use super::app_lifecycle::AppLifecycle;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_request::DistributionRequest;
use super::window_capture_mode::WindowCaptureMode;

pub struct DistributionRecoveryAuditService;

impl DistributionRecoveryAuditService {
    pub fn restore_and_recover(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        pid: u32,
        targets: &[String],
        capture_mode: WindowCaptureMode,
        operation_id: &str,
        request: &DistributionRequest,
    ) -> Result<(), String> {
        let trigger = request.trigger.as_str();
        let reason = request.reason.as_str();
        if capture_mode == WindowCaptureMode::Captured {
            if let Err(error) =
                Self::restore_window_bounds(logger, lifecycle, pid, operation_id, trigger, reason)
            {
                lifecycle.abort_recovery();
                return Err(error);
            }
        }
        Self::recover_and_verify(
            logger,
            lifecycle,
            targets,
            &[pid],
            capture_mode == WindowCaptureMode::Captured,
            operation_id,
            request,
        )
    }

    pub fn capture_window_bounds(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        operation_id: &str,
        trigger: &str,
        reason: &str,
        targets: &[String],
    ) -> Result<WindowCaptureMode, String> {
        match lifecycle.capture_window_bounds(operation_id, targets, reason) {
            Ok(mode) => {
                logger.log_action(
                    operation_id,
                    match mode {
                        WindowCaptureMode::Captured => "WINDOW_CAPTURED",
                        WindowCaptureMode::Absent => "WINDOW_ABSENT",
                    },
                    trigger,
                    reason,
                    match mode {
                        WindowCaptureMode::Captured => "Desktop window frame and process identity captured",
                        WindowCaptureMode::Absent => "Desktop has no eligible window; recovery will use Desktop IPC without geometry restore",
                    },
                );
                Ok(mode)
            }
            Err(error) => {
                logger.log_failure(
                    operation_id,
                    "WINDOW_CAPTURE_FAILED",
                    trigger,
                    reason,
                    &format!("Desktop window capture failed: {error}"),
                );
                Err(error)
            }
        }
    }

    pub fn restore_window_bounds(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        pid: u32,
        operation_id: &str,
        trigger: &str,
        reason: &str,
    ) -> Result<(), String> {
        match lifecycle.restore_window_bounds(pid, operation_id, reason) {
            Ok(()) => {
                logger.log_action(
                    operation_id,
                    "WINDOW_RESTORED",
                    trigger,
                    reason,
                    "Desktop window frame verified on the new process",
                );
                Ok(())
            }
            Err(error) => {
                logger.log_failure(
                    operation_id,
                    "WINDOW_RESTORE_FAILED",
                    trigger,
                    reason,
                    &format!("Desktop window restore failed: {error}"),
                );
                Err(error)
            }
        }
    }

    pub fn recover_and_verify(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        targets: &[String],
        pids: &[u32],
        require_window: bool,
        operation_id: &str,
        request: &DistributionRequest,
    ) -> Result<(), String> {
        let trigger = request.trigger.as_str();
        let reason = request.reason.as_str();
        logger.log_action(
            operation_id,
            "RECOVERY_START",
            trigger,
            reason,
            "Starting Desktop-owned recovery verification",
        );
        let result = lifecycle
            .recover_threads(targets)
            .and_then(|_| lifecycle.verify_desktop_stable(pids, require_window));
        if result.is_ok() {
            logger.log_action(
                operation_id,
                "RECOVERY_VERIFIED",
                trigger,
                reason,
                "Desktop-owned recovery verified",
            );
        } else {
            logger.log_warning(
                operation_id,
                "RECOVERY_INCOMPLETE",
                trigger,
                reason,
                "Desktop recovery verification incomplete",
            );
        }
        result
    }
}
