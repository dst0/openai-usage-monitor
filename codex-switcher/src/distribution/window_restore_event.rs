use super::window_restore_outcome::RestoreOutcome;
use super::window_restore_sanitizer::{sanitize_operation_id, sanitize_text};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreEvent {
    pub operation_id: String,
    pub phase: String,
    pub reason: String,
    pub outcome: RestoreOutcome,
    pub detail: String,
}

impl RestoreEvent {
    pub fn new(
        operation_id: &str,
        phase: &str,
        reason: &str,
        outcome: RestoreOutcome,
        detail: &str,
    ) -> Self {
        Self {
            operation_id: sanitize_operation_id(operation_id),
            phase: sanitize_text(phase),
            reason: sanitize_text(reason),
            outcome,
            detail: sanitize_text(detail),
        }
    }
}
