use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use crate::models::{AccountConfig, WhamUsageResponse};
use crate::quota::ResetCreditConsumeOutcome;
use crate::state_file::fake_state_file_operations::FakeStateFileOperations;
use crate::state_file::{StateFileOperations, SystemStateFileOperations};
use std::cell::{Cell, RefCell};

/// `(journal state on disk, key on disk, key sent)` when a request leaves.
pub(super) type ObservedRequest = (String, Option<String>, String);

/// Deterministic host for automatic reset tests. It never contacts the
/// network, the process table, Desktop, or a Desktop thread database, and its
/// journal files are real but can be scripted to fail.
pub(super) struct FakeWeeklyResetEnvironment {
    blocked_threads: Vec<String>,
    desktop: Result<bool, String>,
    outcome: ResetCreditConsumeOutcome,
    usage: Result<WhamUsageResponse, String>,
    recovery: Result<(), String>,
    journal_files: FakeStateFileOperations,
    during_detection: RefCell<Option<Box<dyn FnOnce()>>>,
    detections: Cell<usize>,
    requests: RefCell<Vec<ObservedRequest>>,
    sent_accounts: RefCell<Vec<AccountConfig>>,
    usage_reads: RefCell<Vec<(String, AccountConfig)>>,
    recoveries: RefCell<Vec<(String, Vec<String>)>>,
}

impl FakeWeeklyResetEnvironment {
    pub(super) fn new(blocked_threads: &[&str]) -> Self {
        Self {
            blocked_threads: blocked_threads.iter().map(|id| id.to_string()).collect(),
            desktop: Ok(true),
            outcome: ResetCreditConsumeOutcome::Unknown("fake_outcome_not_configured".into()),
            usage: Err("fake_usage_not_configured".into()),
            recovery: Ok(()),
            journal_files: FakeStateFileOperations::new(),
            during_detection: RefCell::new(None),
            detections: Cell::new(0),
            requests: RefCell::new(Vec::new()),
            sent_accounts: RefCell::new(Vec::new()),
            usage_reads: RefCell::new(Vec::new()),
            recoveries: RefCell::new(Vec::new()),
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

    pub(super) fn with_usage(mut self, usage: Result<WhamUsageResponse, String>) -> Self {
        self.usage = usage;
        self
    }

    pub(super) fn with_recovery(mut self, recovery: Result<(), String>) -> Self {
        self.recovery = recovery;
        self
    }

    pub(super) fn with_journal_files(mut self, files: FakeStateFileOperations) -> Self {
        self.journal_files = files;
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

    pub(super) fn detections(&self) -> usize {
        self.detections.get()
    }

    /// The account copies handed to the reset request, in order.
    pub(super) fn sent_accounts(&self) -> Vec<AccountConfig> {
        self.sent_accounts.borrow().clone()
    }

    /// `(journal state on disk, account)` for each post-reset usage read.
    pub(super) fn usage_reads(&self) -> Vec<(String, AccountConfig)> {
        self.usage_reads.borrow().clone()
    }

    /// `(journal state on disk, task IDs)` for each recovery hand-off.
    pub(super) fn recoveries(&self) -> Vec<(String, Vec<String>)> {
        self.recoveries.borrow().clone()
    }

    pub(super) fn journal_fake(&self) -> &FakeStateFileOperations {
        &self.journal_files
    }

    /// Observes the journal with unscripted operations, so observation never
    /// consumes or triggers a scripted fault.
    fn journal_on_disk() -> (String, Option<String>) {
        match ResetJournalStore::new(&SystemStateFileOperations).load() {
            Ok(journal) => (journal.state, journal.idempotency_key),
            Err(error) => (format!("unreadable: {error}"), None),
        }
    }
}

impl WeeklyResetEnvironment for FakeWeeklyResetEnvironment {
    fn quota_blocked_threads(&self) -> Vec<String> {
        self.detections.set(self.detections.get() + 1);
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
        account: &AccountConfig,
        idempotency_key: &str,
    ) -> ResetCreditConsumeOutcome {
        self.sent_accounts.borrow_mut().push(account.clone());
        let (state, key) = Self::journal_on_disk();
        self.requests
            .borrow_mut()
            .push((state, key, idempotency_key.to_string()));
        self.outcome.clone()
    }

    fn read_usage(&self, account: &AccountConfig) -> Result<WhamUsageResponse, String> {
        let (state, _) = Self::journal_on_disk();
        self.usage_reads.borrow_mut().push((state, account.clone()));
        self.usage.clone()
    }

    fn recover_threads(&self, thread_ids: &[String]) -> Result<(), String> {
        let (state, _) = Self::journal_on_disk();
        self.recoveries
            .borrow_mut()
            .push((state, thread_ids.to_vec()));
        self.recovery.clone()
    }

    fn journal_files(&self) -> &dyn StateFileOperations {
        &self.journal_files
    }
}
