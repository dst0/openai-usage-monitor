use super::app_lifecycle::AppLifecycle;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_request::DistributionRequest;
use super::recovery_audit_context::RecoveryAuditContext;
use super::window_capture_mode::WindowCaptureMode;

pub struct DistributionRecoveryAuditService;

impl DistributionRecoveryAuditService {
    pub(super) fn restore_and_recover(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        context: RecoveryAuditContext<'_>,
        before_recovery: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        let trigger = context.request.trigger.as_str();
        let reason = context.request.reason.as_str();
        if context.capture_mode == WindowCaptureMode::Captured {
            if let Err(error) = Self::restore_window_bounds(
                logger,
                lifecycle,
                context.pid,
                context.operation_id,
                trigger,
                reason,
            ) {
                lifecycle.abort_recovery();
                return Err(error);
            }
        } else if context.capture_mode == WindowCaptureMode::Skipped {
            if let Err(error) = lifecycle.rebind_banner(context.pid) {
                logger.log_warning(
                    context.operation_id,
                    "RECOVERY_BANNER_REBIND_FAILED",
                    trigger,
                    reason,
                    &format!("Recovery banner could not follow relaunched Desktop: {error}"),
                );
            }
        }
        if let Err(error) = before_recovery() {
            lifecycle.abort_recovery();
            return Err(error);
        }
        Self::recover_and_verify(
            logger,
            lifecycle,
            context.targets,
            &[context.pid],
            context.capture_mode == WindowCaptureMode::Captured,
            context.operation_id,
            context.request,
        )
    }

    pub fn capture_window_bounds(
        logger: &DistributionAuditLogger,
        lifecycle: &dyn AppLifecycle,
        operation_id: &str,
        trigger: &str,
        reason: &str,
        targets: &[String],
        preserve_window_bounds: bool,
    ) -> Result<WindowCaptureMode, String> {
        match lifecycle.capture_window_bounds(operation_id, targets, reason, preserve_window_bounds)
        {
            Ok(mode) => {
                logger.log_action(
                    operation_id,
                    match mode {
                        WindowCaptureMode::Captured => "WINDOW_CAPTURED",
                        WindowCaptureMode::Absent => "WINDOW_ABSENT",
                        WindowCaptureMode::Skipped => "WINDOW_CAPTURE_SKIPPED",
                    },
                    trigger,
                    reason,
                    match mode {
                        WindowCaptureMode::Captured => "Desktop window frame and process identity captured",
                        WindowCaptureMode::Absent => "Desktop has no eligible window; recovery will use Desktop IPC without geometry restore",
                        WindowCaptureMode::Skipped => "Window preservation disabled; recovery will use Desktop IPC without geometry restore",
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
