use super::reset_journal::ResetJournal;
use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use super::weekly_reset_policy::{now_string, report, weekly_reset_reflected};
use super::AutoResetReport;
use crate::models::AccountConfig;

/// Settles a reset the service reported as applied: records `applied`, reads
/// fresh usage as the official contract requires (which also lets recovery
/// observe the restored pool), and hands the blocked tasks to Desktop.
pub(super) struct AppliedResetSettlementService<'a, E: WeeklyResetEnvironment> {
    environment: &'a E,
    store: &'a ResetJournalStore<'a>,
}

impl<'a, E: WeeklyResetEnvironment> AppliedResetSettlementService<'a, E> {
    pub(super) fn new(environment: &'a E, store: &'a ResetJournalStore<'a>) -> Self {
        Self { environment, store }
    }

    pub(super) fn settle(
        &self,
        mut journal: ResetJournal,
        blocked_threads: &[String],
        latest_active: &AccountConfig,
    ) -> Result<AutoResetReport, String> {
        journal.state = "applied".into();
        journal.reason = None;
        journal.updated_at = Some(now_string());
        // If `applied` never replaced `pending`, the same-key retry receives
        // the service's idempotent success and recovers then. If it is visible
        // but was not flushed, the next tick sees `applied` and never retries,
        // so the paid-for hand-off must happen now and the write is repeated.
        let unflushed = match self.store.write(&journal) {
            Ok(()) => false,
            Err(_) if self.store.load().is_ok_and(|saved| saved == journal) => true,
            Err(error) => return Err(error),
        };
        let quota_refresh_reason = match self.environment.read_usage(latest_active) {
            Ok(usage)
                if usage.account_id.as_deref() == Some(latest_active.account_id.as_str())
                    && weekly_reset_reflected(&usage) == Some(true) =>
            {
                None
            }
            Ok(_) => Some("reset_applied_quota_refresh_unverified".to_string()),
            Err(_) => Some("reset_applied_quota_refresh_failed".to_string()),
        };
        let recovery_reason = self
            .environment
            .recover_threads(blocked_threads)
            .err()
            .map(|reason| format!("reset_applied_recovery_unverified:{reason}"));
        journal.reason = match (quota_refresh_reason, recovery_reason) {
            (None, None) => None,
            (Some(reason), None) | (None, Some(reason)) => Some(reason),
            (Some(quota_reason), Some(recovery_reason)) => {
                Some(format!("{quota_reason};{recovery_reason}"))
            }
        };
        if unflushed || journal.reason.is_some() {
            journal.updated_at = Some(now_string());
            self.store.write(&journal)?;
        }
        Ok(report(
            journal.state,
            journal.reason,
            journal.updated_at,
            true,
        ))
    }
}
