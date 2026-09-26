use super::{
    manifest_store::{load_manifest, write_manifest},
    pending_target::PendingTarget,
};

/// The recovery journal state before a restart attempt mutates it. Callers
/// hold the global operation lock until either shutdown starts or rollback
/// completes, so this snapshot cannot overwrite a concurrent recovery write.
pub(crate) struct RecoveryManifestSnapshot {
    targets: Vec<PendingTarget>,
}

impl RecoveryManifestSnapshot {
    pub(crate) fn capture() -> Result<Self, String> {
        Ok(Self {
            targets: load_manifest()?,
        })
    }

    pub(crate) fn restore(&self) -> Result<(), String> {
        write_manifest(&self.targets)
    }

    pub(crate) fn rollback_error(&self, cause: String) -> String {
        match self.restore() {
            Ok(()) => cause,
            Err(rollback) => {
                format!("{cause}; prior recovery journal could not be restored: {rollback}")
            }
        }
    }
}
