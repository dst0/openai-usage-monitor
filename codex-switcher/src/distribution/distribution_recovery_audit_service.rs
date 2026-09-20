use super::app_lifecycle::AppLifecycle;
use super::distribution_audit_logger::DistributionAuditLogger;

pub struct DistributionRecoveryAuditService;

impl DistributionRecoveryAuditService {
    pub fn capture_window_bounds(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        operation_id: &str,
        trigger: &str,
        reason: &str,
        targets: &[String],
    ) -> Result<(), String> {
        match lifecycle.capture_window_bounds(operation_id, targets, reason) {
            Ok(()) => {
                logger.log_action(
                    operation_id,
                    "WINDOW_CAPTURED",
                    trigger,
                    reason,
                    "Desktop window frame and process identity captured",
                );
                Ok(())
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
        operation_id: &str,
        trigger: &str,
        reason: &str,
    ) -> Result<(), String> {
        logger.log_action(
            operation_id,
            "RECOVERY_START",
            trigger,
            reason,
            "Starting Desktop-owned recovery verification",
        );
        let result = lifecycle
            .recover_threads(targets)
            .and_then(|_| lifecycle.verify_desktop_stable(pids));
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
