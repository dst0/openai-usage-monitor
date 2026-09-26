use super::{
    dispatch_mark_error::DispatchMarkError,
    manifest_store::{mark_dispatch_attempt, restore_undispatched_target},
    recovery_mode::RecoveryMode,
    recovery_target::RecoveryTarget,
};

pub(super) struct RecoveryDispatchCheckpointService;

impl RecoveryDispatchCheckpointService {
    pub(super) fn mark_and_confirm(
        target: &mut RecoveryTarget,
        mode: RecoveryMode,
        mut verify_identity: impl FnMut() -> Result<(), DispatchMarkError>,
    ) -> Result<(), String> {
        let original = match mark_dispatch_attempt(&target.id, mode, &mut verify_identity) {
            Ok(original) => original,
            Err(error) => {
                if matches!(error, DispatchMarkError::AccountChanged) {
                    target.account_mismatch = true;
                }
                return Err(error.to_string());
            }
        };
        // A manual Desktop/menu switch does not participate in Monitor's
        // operation lock. Recheck after the manifest rename and fsync, before
        // any IPC bytes can leave this process. This is still a pre-send
        // failure, so the exact original checkpoint can be restored.
        if let Err(error) = verify_identity() {
            if matches!(error, DispatchMarkError::AccountChanged) {
                target.account_mismatch = true;
            }
            return match restore_undispatched_target(&original) {
                Ok(()) => Err(error.to_string()),
                Err(restore) => Err(format!(
                    "{error}; original recovery checkpoint could not be restored: {restore}"
                )),
            };
        }
        Ok(())
    }
}
