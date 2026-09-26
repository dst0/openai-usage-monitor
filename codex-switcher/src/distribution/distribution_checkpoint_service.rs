use super::app_lifecycle::AppLifecycle;
use super::distribution_journal::DistributionJournal;
use crate::recovery::{self, RecoveryManifestSnapshot};
use std::path::Path;

pub struct DistributionCheckpointService;

impl DistributionCheckpointService {
    /// The first window check precedes any journal write. The second catches
    /// a window added while `save_pending` was writing; rejection restores
    /// the exact previously eligible target set under the operation lock.
    pub(crate) fn prepare(
        home: &Path,
        lifecycle: &dyn AppLifecycle,
        targets: &[String],
    ) -> Result<RecoveryManifestSnapshot, String> {
        if let Err(error) = lifecycle.preflight_shutdown_windows() {
            return Err(Self::clear_unmodified_journal(home, error));
        }
        let snapshot = RecoveryManifestSnapshot::capture()?;
        recovery::save_pending(targets)
            .map_err(|error| Self::rollback_and_clear(home, &snapshot, error))?;
        lifecycle
            .preflight_shutdown_windows()
            .map_err(|error| Self::rollback_and_clear(home, &snapshot, error))?;
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
    pub(crate) fn finalize_after_stop(targets: &[String]) -> Result<(), String> {
        recovery::save_pending(targets)
            .map_err(|error| format!("Post-shutdown recovery checkpoint failed: {error}"))
    }
}
