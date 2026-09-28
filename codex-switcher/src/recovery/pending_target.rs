use super::auth_rotation_recovery_evidence::AuthRotationRecoveryEvidence;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(super) struct PendingTarget {
    pub(super) id: String,
    pub(super) offset: Option<u64>,
    #[serde(default)]
    pub(super) awaiting_owner: bool,
    #[serde(default)]
    pub(super) captured_restart: bool,
    #[serde(default)]
    pub(super) owner_account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) auth_rotation: Option<AuthRotationRecoveryEvidence>,
}
