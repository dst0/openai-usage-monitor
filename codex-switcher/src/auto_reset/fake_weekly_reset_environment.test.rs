use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use crate::models::AccountConfig;
use crate::quota::ResetCreditConsumeOutcome;
use std::cell::RefCell;

/// `(journal state on disk, key on disk, key sent)` when a request leaves.
pub(super) type ObservedRequest = (String, Option<String>, String);

/// Deterministic host for automatic reset tests. It never contacts the
/// network, the process table, or a Desktop thread database.
pub(super) struct FakeWeeklyResetEnvironment {
    blocked_threads: Vec<String>,
    desktop: Result<bool, String>,
    outcome: ResetCreditConsumeOutcome,
    during_detection: RefCell<Option<Box<dyn FnOnce()>>>,
    requests: RefCell<Vec<ObservedRequest>>,
}

impl FakeWeeklyResetEnvironment {
    pub(super) fn new(blocked_threads: &[&str]) -> Self {
        Self {
            blocked_threads: blocked_threads.iter().map(|id| id.to_string()).collect(),
            desktop: Ok(true),
            outcome: ResetCreditConsumeOutcome::Unknown("fake_outcome_not_configured".into()),
            during_detection: RefCell::new(None),
            requests: RefCell::new(Vec::new()),
        }
    }

    pub(super) fn with_desktop(mut self, desktop: Result<bool, String>) -> Self {
        self.desktop = desktop;
        self
    }

    pub(super) fn with_outcome(mut self, outcome: ResetCreditConsumeOutcome) -> Self {
        self.outcome = outcome;
        self
    }

    /// Runs once while tasks are detected: after the lock-time registry check
    /// and before the final preflight, which is where a concurrent account,
    /// quota, policy, or auth change lands in production.
    pub(super) fn during_detection(self, change: impl FnOnce() + 'static) -> Self {
        *self.during_detection.borrow_mut() = Some(Box::new(change));
        self
    }

    pub(super) fn requests(&self) -> Vec<ObservedRequest> {
        self.requests.borrow().clone()
    }
}

impl WeeklyResetEnvironment for FakeWeeklyResetEnvironment {
    fn quota_blocked_threads(&self) -> Vec<String> {
        if let Some(change) = self.during_detection.borrow_mut().take() {
            change();
        }
        self.blocked_threads.clone()
    }

    fn desktop_running(&self) -> Result<bool, String> {
        self.desktop.clone()
    }

    fn consume_reset_credit(
        &self,
        _account: &AccountConfig,
        idempotency_key: &str,
    ) -> ResetCreditConsumeOutcome {
        let (state, key) = match ResetJournalStore::load() {
            Ok(journal) => (journal.state, journal.idempotency_key),
            Err(error) => (format!("unreadable: {error}"), None),
        };
        self.requests
            .borrow_mut()
            .push((state, key, idempotency_key.to_string()));
        self.outcome.clone()
    }
}
