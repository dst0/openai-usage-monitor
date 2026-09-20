use super::window_restore_event::RestoreEvent;
use super::window_restore_outcome::RestoreOutcome;
use super::window_restore_sanitizer::{sanitize_operation_id, sanitize_text};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RestoreReport {
    pub operation_id: String,
    pub reason: String,
    pub outcome: RestoreOutcome,
    pub events: Vec<RestoreEvent>,
}

impl RestoreReport {
    pub fn new(operation_id: &str, reason: &str) -> Self {
        Self {
            operation_id: sanitize_operation_id(operation_id),
            reason: sanitize_text(reason),
            outcome: RestoreOutcome::InProgress,
            events: Vec::new(),
        }
    }

    pub fn record(&mut self, phase: &str, outcome: RestoreOutcome, detail: &str) {
        self.events.push(RestoreEvent::new(
            &self.operation_id,
            phase,
            &self.reason,
            outcome,
            detail,
        ));
    }
}
