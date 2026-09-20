use serde::{Deserialize, Serialize};

pub(super) const JOURNAL_VERSION: u8 = 1;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct ResetJournal {
    pub(super) version: u8,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) episode_key: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) account_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) thread_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) idempotency_key: Option<String>,
    #[serde(default)]
    pub(super) state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) updated_at: Option<String>,
}

impl Default for ResetJournal {
    fn default() -> Self {
        Self {
            version: JOURNAL_VERSION,
            episode_key: None,
            account_id: None,
            thread_id: None,
            idempotency_key: None,
            state: "ready".into(),
            reason: None,
            updated_at: None,
        }
    }
}
