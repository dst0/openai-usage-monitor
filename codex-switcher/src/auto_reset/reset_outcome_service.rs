use super::reset_journal::ResetJournal;
use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_policy::{now_string, report, weekly_reset_reflected};
use super::AutoResetReport;
use crate::models::AccountConfig;
use crate::{quota, recovery};

pub(super) struct ResetOutcomeService;

impl ResetOutcomeService {
    pub(super) fn handle(
        outcome: quota::ResetCreditConsumeOutcome,
        mut journal: ResetJournal,
        blocked_threads: &[String],
        latest_active: &AccountConfig,
    ) -> Result<AutoResetReport, String> {
        match outcome {
            quota::ResetCreditConsumeOutcome::Applied => {
                journal.state = "applied".into();
                journal.reason = None;
                journal.updated_at = Some(now_string());
                ResetJournalStore::write(&journal)?;
                // The service outcome is authoritative for idempotency, but the
                // official contract requires a fresh limits read afterwards. This
                // read also gives recovery a chance to observe the restored pool.
                let mut refreshed_account = latest_active.clone();
                let quota_refresh_reason =
                    match quota::fetch_account_usage_read_only(&mut refreshed_account) {
                        Ok(usage)
                            if usage.account_id.as_deref()
                                == Some(latest_active.account_id.as_str())
                                && weekly_reset_reflected(&usage) == Some(true) =>
                        {
                            None
                        }
                        Ok(_) => Some("reset_applied_quota_refresh_unverified".to_string()),
                        Err(_) => Some("reset_applied_quota_refresh_failed".to_string()),
                    };
                let recovery_reason = recovery::recover_threads(
                    blocked_threads,
                    recovery::RecoveryMode::DiscoveredOnly,
                )
                .err()
                .map(|reason| format!("reset_applied_recovery_unverified:{reason}"));
                journal.reason = match (quota_refresh_reason, recovery_reason) {
                    (None, None) => None,
                    (Some(reason), None) | (None, Some(reason)) => Some(reason),
                    (Some(quota_reason), Some(recovery_reason)) => {
                        Some(format!("{quota_reason};{recovery_reason}"))
                    }
                };
                if journal.reason.is_some() {
                    journal.updated_at = Some(now_string());
                    ResetJournalStore::write(&journal)?;
                }
                Ok(report(
                    journal.state,
                    journal.reason,
                    journal.updated_at,
                    true,
                ))
            }
            quota::ResetCreditConsumeOutcome::NotConsumed(reason) => {
                journal.state = "not_consumed".into();
                journal.reason = Some(reason);
                journal.updated_at = Some(now_string());
                ResetJournalStore::write(&journal)?;
                Ok(report(
                    journal.state,
                    journal.reason,
                    journal.updated_at,
                    false,
                ))
            }
            quota::ResetCreditConsumeOutcome::Unavailable(reason) => {
                journal.state = "waiting_for_service".into();
                journal.reason = Some(reason);
                journal.updated_at = Some(now_string());
                ResetJournalStore::write(&journal)?;
                Ok(report(
                    journal.state,
                    journal.reason,
                    journal.updated_at,
                    false,
                ))
            }
            quota::ResetCreditConsumeOutcome::Unknown(reason) => {
                journal.state = "unknown".into();
                journal.reason = Some(reason);
                journal.updated_at = Some(now_string());
                ResetJournalStore::write(&journal)?;
                Ok(report(
                    journal.state,
                    journal.reason,
                    journal.updated_at,
                    true,
                ))
            }
        }
    }
}
