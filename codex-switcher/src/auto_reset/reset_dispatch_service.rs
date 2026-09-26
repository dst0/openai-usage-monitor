use super::reset_journal::ResetJournal;
use super::reset_journal_store::ResetJournalStore;
use super::reset_outcome_service::ResetOutcomeService;
use super::reset_preflight::ResetPreflight;
use super::reset_preflight_service::ResetPreflightService;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use super::weekly_reset_policy::{now_string, report, unresolved_attempt};
use super::AutoResetReport;
use crate::models::{AccountConfig, Settings};
use crate::quota::ResetCreditConsumeOutcome;

/// The only place an automatic attempt becomes `pending` and a reset request
/// is sent. All eligibility checks run first; the durable `pending` marker is
/// then followed directly by the request, so no exit can strand an unsent
/// attempt as unresolved. An attempt that may already have reached the
/// service keeps its unresolved journal on every refusal.
pub(super) struct ResetDispatchService;

impl ResetDispatchService {
    /// `journal` is either the persisted attempt for this episode (a retry
    /// reusing its key) or a new in-memory attempt that has not been written.
    pub(super) fn dispatch(
        settings: &Settings,
        active: &AccountConfig,
        environment: &impl WeeklyResetEnvironment,
        mut journal: ResetJournal,
        persisted: bool,
        blocked_threads: &[String],
    ) -> Result<AutoResetReport, String> {
        let idempotency_key = journal
            .idempotency_key
            .clone()
            .ok_or("Auto-reset journal has no idempotency key")?;
        let outcome_may_be_unknown = persisted && unresolved_attempt(&journal);
        let latest_active =
            match ResetPreflightService::check(settings, active, &idempotency_key, environment)? {
                ResetPreflight::Ready(latest_active) => latest_active,
                // A request with this key may already have reached the service.
                // Keep the sole journal unresolved and rotation suppressed.
                ResetPreflight::Refused { .. } if outcome_may_be_unknown => {
                    return Ok(report(
                        journal.state,
                        journal.reason,
                        journal.updated_at,
                        true,
                    ));
                }
                ResetPreflight::Refused {
                    state,
                    reason,
                    record: true,
                } => {
                    journal.state = state;
                    journal.reason = reason;
                    journal.updated_at = Some(now_string());
                    ResetJournalStore::write(&journal)?;
                    return Ok(report(
                        journal.state,
                        journal.reason,
                        journal.updated_at,
                        false,
                    ));
                }
                ResetPreflight::Refused { state, reason, .. } => {
                    return Ok(report(state, reason, journal.updated_at, false));
                }
            };
        if !outcome_may_be_unknown {
            // Persist before dispatch. A crash after the request is sent must
            // retry this exact operation rather than mint a new credit use.
            journal.state = "pending".into();
            journal.reason = None;
            journal.updated_at = Some(now_string());
            ResetJournalStore::write(&journal)?;
        }
        let outcome = environment.consume_reset_credit(&latest_active, &idempotency_key);
        if outcome_may_be_unknown && matches!(outcome, ResetCreditConsumeOutcome::Unavailable(_)) {
            // This retry never left the host, but the earlier request with the
            // same key may have been applied. Keep it unresolved.
            return Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                true,
            ));
        }
        ResetOutcomeService::handle(outcome, journal, blocked_threads, &latest_active)
    }
}
