use super::auth_rotation_queue_snapshot::AuthRotationQueueSnapshot;
use super::dispatch_mark_error::DispatchMarkError;
use serde::{Deserialize, Serialize};
use std::path::Path;

/// An operation-bound exception for an active turn that failed while the
/// Monitor was changing from one verified Desktop account to another.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct AuthRotationRecoveryEvidence {
    pub(super) source_account_id: String,
    pub(super) target_account_id: String,
    pub(super) pre_stop_offset: u64,
    pub(super) rollout_dev: u64,
    pub(super) rollout_ino: u64,
    pub(super) turn_id: String,
    pub(super) queue_snapshot: AuthRotationQueueSnapshot,
    pub(super) confirmed_after_stop: bool,
}

impl AuthRotationRecoveryEvidence {
    pub(super) fn eligible_for(&self, home: &Path, id: &str, account_id: &str) -> bool {
        self.confirmed_after_stop
            && self.target_account_id == account_id
            && self.queue_still_current(home, id)
    }

    fn queue_still_current(&self, home: &Path, id: &str) -> bool {
        AuthRotationQueueSnapshot::read(home, id).ok().as_ref() == Some(&self.queue_snapshot)
    }

    pub(super) fn require_queue_current(
        &self,
        home: &Path,
        id: &str,
    ) -> Result<(), DispatchMarkError> {
        self.queue_still_current(home, id)
            .then_some(())
            .ok_or_else(|| {
                DispatchMarkError::Other("Recovery queue changed since account switch".into())
            })
    }
}
