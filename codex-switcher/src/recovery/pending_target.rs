use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Deserialize, Serialize)]
pub(super) struct PendingTarget {
    pub(super) id: String,
    pub(super) offset: Option<u64>,
}
