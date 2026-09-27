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
        // The credit is spent, so the paid-for hand-off must not depend on the
        // journal: no later tick repeats it. A visible `applied` returns early,
        // and a `pending` that `applied` never replaced is held once the fresh
        // quota shows the restored pool. A failed write is repeated afterwards.
        let first_write = self.store.write(&journal);
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
        if first_write.is_err() || journal.reason.is_some() {
            journal.updated_at = Some(now_string());
            self.store.write(&journal).map_err(|error| {
                format!(
                    "{error}; the reset credit was applied and its tasks were handed to recovery"
                )
            })?;
        }
        Ok(report(
            journal.state,
            journal.reason,
            journal.updated_at,
            true,
        ))
    }
}
