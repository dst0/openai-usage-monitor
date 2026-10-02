use super::auth_rotation_checkpoint_service::AuthRotationCheckpointService;
use super::auth_rotation_queue_snapshot::AuthRotationQueueSnapshot;
use super::dispatch_mark_error::DispatchMarkError;
use crate::switcher::{self, ThreadRolloutState};
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
    pub(super) fn eligible_for(
        &self,
        home: &Path,
        id: &str,
        account_id: &str,
        final_offset: Option<u64>,
    ) -> bool {
        self.confirmed_after_stop
            && self.target_account_id == account_id
            && self.still_current(home, id, final_offset)
    }

    fn queue_still_current(&self, home: &Path, id: &str) -> bool {
        AuthRotationQueueSnapshot::read(home, id).ok().as_ref() == Some(&self.queue_snapshot)
    }

    fn still_current(&self, home: &Path, id: &str, final_offset: Option<u64>) -> bool {
        self.queue_still_current(home, id)
            && final_offset.is_some_and(|offset| {
                AuthRotationCheckpointService::interval_confirmed(
                    home,
                    id,
                    self.pre_stop_offset,
                    offset,
                    &self.turn_id,
                    self.rollout_dev,
                    self.rollout_ino,
                )
            })
            && switcher::inspect_thread_rollout_state(home, id)
                == ThreadRolloutState::InterruptedByError
    }

    pub(super) fn require_current(
        &self,
        home: &Path,
        id: &str,
        final_offset: Option<u64>,
    ) -> Result<(), DispatchMarkError> {
        self.still_current(home, id, final_offset)
            .then_some(())
            .ok_or_else(|| {
                DispatchMarkError::Other(
                    "Auth recovery evidence changed since account switch".into(),
                )
            })
    }
}
