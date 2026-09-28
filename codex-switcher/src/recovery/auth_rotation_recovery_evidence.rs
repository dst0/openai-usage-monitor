use serde::{Deserialize, Serialize};

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
    pub(super) queue_revision: u64,
    pub(super) confirmed_after_stop: bool,
}
