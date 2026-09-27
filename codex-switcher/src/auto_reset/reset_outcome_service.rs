use super::applied_reset_settlement_service::AppliedResetSettlementService;
use super::reset_journal::ResetJournal;
use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use super::weekly_reset_policy::{now_string, report};
use super::AutoResetReport;
use crate::models::AccountConfig;
use crate::quota;

pub(super) struct ResetOutcomeService;

impl ResetOutcomeService {
    pub(super) fn handle(
        outcome: quota::ResetCreditConsumeOutcome,
        mut journal: ResetJournal,
        blocked_threads: &[String],
        latest_active: &AccountConfig,
        environment: &impl WeeklyResetEnvironment,
    ) -> Result<AutoResetReport, String> {
        let store = ResetJournalStore::new(environment.journal_files());
        match outcome {
            quota::ResetCreditConsumeOutcome::Applied => AppliedResetSettlementService::new(
                environment,
                &store,
            )
            .settle(journal, blocked_threads, latest_active),
            quota::ResetCreditConsumeOutcome::NotConsumed(reason) => {
                journal.state = "not_consumed".into();
                journal.reason = Some(reason);
                journal.updated_at = Some(now_string());
                store.write(&journal)?;
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
                store.write(&journal)?;
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
                store.write(&journal)?;
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
