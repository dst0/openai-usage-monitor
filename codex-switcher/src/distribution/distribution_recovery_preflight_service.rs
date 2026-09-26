use super::{
    app_lifecycle::AppLifecycle, distribution_audit_logger::DistributionAuditLogger,
    distribution_checkpoint_service::DistributionCheckpointService,
    distribution_request::DistributionRequest,
};
use crate::recovery::{self, RecoveryManifestSnapshot};
use std::path::Path;

pub(super) struct DistributionRecoveryPreflightService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
    dispatch: fn() -> Result<(), String>,
}

impl<'a> DistributionRecoveryPreflightService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
    ) -> Self {
        Self {
            lifecycle,
            logger,
            dispatch: recovery::preflight_desktop_dispatch,
        }
    }

    #[cfg(test)]
    pub(super) fn with_dispatch(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
        dispatch: fn() -> Result<(), String>,
    ) -> Self {
        Self {
            lifecycle,
            logger,
            dispatch,
        }
    }

    pub(super) fn run(
        &self,
        home: &Path,
        checkpoint: &RecoveryManifestSnapshot,
        running_threads: &[String],
        operation_id: &str,
        request: &DistributionRequest,
    ) -> Result<(), String> {
        if running_threads.is_empty() {
            return Ok(());
        }
        let Err(error) = (self.dispatch)() else {
            return Ok(());
        };
        self.lifecycle.abort_recovery();
        self.logger.log_failure(
            operation_id,
            "RECOVERY_PREFLIGHT_FAILED",
            request.trigger.as_str(),
            &request.reason,
            "Desktop recovery channel was unavailable before shutdown",
        );
        let message = format!("Desktop recovery channel preflight failed: {error}");
        Err(DistributionCheckpointService::rollback_and_clear(
            home, checkpoint, message,
        ))
    }
}
