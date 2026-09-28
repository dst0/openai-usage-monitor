use super::app_lifecycle::AppLifecycle;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_journal::DistributionJournal;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_error::DistributionTransactionError;
use std::path::Path;

pub(super) struct DistributionWindowTaskLifecycleService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
}

impl<'a> DistributionWindowTaskLifecycleService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
    ) -> Self {
        Self { lifecycle, logger }
    }

    pub(super) fn capture(
        &self,
        home: &Path,
        operation_id: &str,
        request: &DistributionRequest,
    ) -> Result<(), DistributionTransactionError> {
        if let Err(error) = self.lifecycle.capture_window_tasks() {
            self.lifecycle.abort_recovery();
            let cleanup = DistributionJournal::clear(home).err();
            let message = match cleanup {
                Some(cleanup) => format!(
                    "Could not capture Desktop window tasks before shutdown: {error}; journal cleanup failed: {cleanup}"
                ),
                None => format!("Could not capture Desktop window tasks before shutdown: {error}"),
            };
            self.logger.log_failure(
                operation_id,
                "WINDOW_TASK_CAPTURE_FAILED",
                request.trigger.as_str(),
                &request.reason,
                &format!("Desktop was not signalled: {message}"),
            );
            return Err(DistributionTransactionError::pre_signal(
                "WINDOW_TASK_CAPTURE_FAILED",
                message,
            ));
        }
        Ok(())
    }

    pub(super) fn finish_with_recovery_error(
        &self,
        recovery_error: Option<String>,
    ) -> Option<String> {
        match (recovery_error, self.lifecycle.finish_window_tasks().err()) {
            (Some(recovery), Some(windows)) => Some(format!("{recovery}; {windows}")),
            (None, Some(windows)) => Some(windows),
            (recovery, None) => recovery,
        }
    }
}
