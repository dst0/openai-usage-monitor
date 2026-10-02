use super::app_lifecycle::AppLifecycle;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_journal::DistributionJournal;
use super::distribution_pre_signal_failure::DistributionPreSignalFailure;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_error::DistributionTransactionError;
use crate::recovery::{self, RecoveryManifestSnapshot};
use std::path::Path;

/// The multi-window shutdown guard could not prove Desktop safe to stop.
pub(crate) const SHUTDOWN_WINDOW_GUARD_FAILED: &str = "SHUTDOWN_WINDOW_GUARD_FAILED";
/// The pre-shutdown recovery checkpoint could not be captured or saved.
pub(crate) const RECOVERY_CHECKPOINT_FAILED: &str = "RECOVERY_CHECKPOINT_FAILED";

pub struct DistributionCheckpointService;

impl DistributionCheckpointService {
    pub(crate) fn prepare_for_switch(
        home: &Path,
        lifecycle: &dyn AppLifecycle,
        targets: &[String],
        account_ids: (&str, &str),
        logger: &DistributionAuditLogger,
        operation_id: &str,
        request: &DistributionRequest,
    ) -> Result<RecoveryManifestSnapshot, DistributionTransactionError> {
        Self::prepare(home, lifecycle, targets, account_ids.0, account_ids.1).map_err(|error| {
            lifecycle.abort_recovery();
            let _ = lifecycle.finish_window_tasks();
            logger.log_failure(
                operation_id,
                error.phase,
                request.trigger.as_str(),
                &request.reason,
                &format!("Desktop was not signalled: {}", error.message),
            );
            error.into()
        })
    }

    /// The first window check precedes any journal write. The second catches
    /// a window added while `save_pending` was writing; rejection restores
    /// the exact previously eligible target set under the operation lock.
    /// Every failure here precedes the Desktop signal and names its phase.
    pub(crate) fn prepare(
        home: &Path,
        lifecycle: &dyn AppLifecycle,
        targets: &[String],
        source_account_id: &str,
        target_account_id: &str,
    ) -> Result<RecoveryManifestSnapshot, DistributionPreSignalFailure> {
        let failed = |phase, message| DistributionPreSignalFailure { phase, message };
        let guard_failed = |error| failed(SHUTDOWN_WINDOW_GUARD_FAILED, error);
        let checkpoint_failed = |error| failed(RECOVERY_CHECKPOINT_FAILED, error);
        if let Err(error) = lifecycle.preflight_shutdown_windows() {
            return Err(guard_failed(Self::clear_unmodified_journal(home, error)));
        }
        let queue_snapshots =
            recovery::AuthRotationCheckpointService::queue_snapshots(home, targets)
                .map_err(|error| checkpoint_failed(Self::clear_unmodified_journal(home, error)))?;
        let snapshot = RecoveryManifestSnapshot::capture()
            .map_err(|error| checkpoint_failed(Self::clear_unmodified_journal(home, error)))?;
        recovery::save_pending(targets)
            .map_err(|error| checkpoint_failed(Self::rollback_and_clear(home, &snapshot, error)))?;
        recovery::AuthRotationCheckpointService::prepare(
            home,
            targets,
            source_account_id,
            target_account_id,
            &queue_snapshots,
        )
        .map_err(|error| checkpoint_failed(Self::rollback_and_clear(home, &snapshot, error)))?;
        lifecycle
            .preflight_shutdown_windows()
            .map_err(|error| guard_failed(Self::rollback_and_clear(home, &snapshot, error)))?;
        Ok(snapshot)
    }

    pub(crate) fn rollback_and_clear(
        home: &Path,
        checkpoint: &RecoveryManifestSnapshot,
        cause: String,
    ) -> String {
        if let Err(rollback) = checkpoint.restore() {
            return format!("{cause}; prior recovery checkpoint could not be restored: {rollback}");
        }
        Self::clear_unmodified_journal(home, cause)
    }

    fn clear_unmodified_journal(home: &Path, cause: String) -> String {
        match DistributionJournal::clear(home) {
            Ok(()) => cause,
            Err(clear) => format!("{cause}; distribution journal cleanup failed: {clear}"),
        }
    }

    /// The shutdown flushes the old Desktop's final rollout state. The caller
    /// owns the guarded relaunch if this checkpoint fails.
    pub(crate) fn finalize_after_stop(home: &Path, targets: &[String]) -> Result<(), String> {
        recovery::AuthRotationCheckpointService::finalize_after_stop(home, targets)
            .map_err(|error| format!("Post-shutdown recovery checkpoint failed: {error}"))
    }
}
