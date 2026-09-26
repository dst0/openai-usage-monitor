use serde::{Deserialize, Serialize};

const JOURNAL_VERSION: u8 = 1;
const MAX_TARGET_ID_BYTES: usize = 2048;

/// One manual reset request. An unresolved record prevents issuing a new key.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ManualResetAttempt {
    version: u8,
    target_id: String,
    before_credits: u32,
    started_at: String,
    idempotency_key: String,
    state: String,
}

impl ManualResetAttempt {
    pub(super) fn pending(target_id: String, before_credits: u32, idempotency_key: String) -> Self {
        Self {
            version: JOURNAL_VERSION,
            target_id,
            before_credits,
            started_at: chrono::Utc::now().to_rfc3339(),
            idempotency_key,
            state: "pending".into(),
        }
    }

    pub(super) fn is_unresolved(&self) -> bool {
        self.state != "resolved"
    }

    pub(super) fn matches_pending(&self, target_id: &str, credits: u32, key: &str) -> bool {
        self.state == "pending"
            && self.target_id == target_id
            && self.before_credits == credits
            && self.idempotency_key == key
    }

    pub(super) fn mark_unknown(&mut self) {
        self.state = "unknown".into();
    }

    pub(super) fn mark_applied_uncertain(&mut self) {
        self.state = "applied_uncertain".into();
    }

    pub(super) fn mark_resolved(&mut self) {
        self.state = "resolved".into();
    }

    pub(super) fn validate(&self) -> Result<(), String> {
        if self.version != JOURNAL_VERSION
            || self.target_id.is_empty()
            || self.target_id.len() > MAX_TARGET_ID_BYTES
            || self.target_id.chars().any(char::is_control)
            || self.before_credits == 0
            || chrono::DateTime::parse_from_rfc3339(&self.started_at).is_err()
            || !matches!(
                self.state.as_str(),
                "pending" | "unknown" | "applied_uncertain" | "resolved"
            )
        {
            return Err("Manual reset attempt is invalid".into());
        }
        let key = self.idempotency_key.as_bytes();
        if key.len() != 36
            || key.iter().enumerate().any(|(index, byte)| {
                if matches!(index, 8 | 13 | 18 | 23) {
                    *byte != b'-'
                } else {
                    !byte.is_ascii_hexdigit()
                }
            })
        {
            return Err("Manual reset attempt key is invalid".into());
        }
        Ok(())
    }
}
