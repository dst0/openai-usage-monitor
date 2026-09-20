use super::pending_target::PendingTarget;
use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
pub(super) struct PendingManifest {
    pub(super) version: u8,
    pub(super) targets: Vec<PendingTarget>,
}
