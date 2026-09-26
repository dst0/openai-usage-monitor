use super::reset_dispatch_service::ResetDispatchService;
use super::reset_journal::{ResetJournal, JOURNAL_VERSION};
use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use super::weekly_reset_policy::{
    episode_key, new_idempotency_key, report, same_episode, terminal_no_spend_state,
    threshold_eligible, unresolved_attempt, weekly_exhausted,
};
use super::weekly_reset_status_service::WeeklyResetStatusService;
use super::AutoResetReport;
use crate::models::{AccountConfig, Settings};
use crate::{recovery, storage};

pub(super) struct WeeklyResetService;

impl WeeklyResetService {
    /// Applies the account-bound reset policy once. The daemon calls this only
    /// after obtaining fresh quota data; an unresolved result keeps the sole
    /// automatic journal and suppresses rotation until it is reconciled.
    pub(super) fn maybe_consume_weekly_reset(
        settings: &Settings,
        active: &AccountConfig,
        environment: &impl WeeklyResetEnvironment,
    ) -> Result<AutoResetReport, String> {
        if !settings.auto_reset_weekly_enabled {
            return Ok(report("disabled", None, None, false));
        }
        let initial_journal = ResetJournalStore::load()?;
        if unresolved_attempt(&initial_journal)
            && (!same_episode(&initial_journal, active)
                || !weekly_exhausted(active)
                || active.last_error.is_some()
                || active.last_credits.unwrap_or(0) == 0
                || !threshold_eligible(active, settings.auto_reset_weekly_min_remaining_seconds)?)
        {
            return Ok(report(
                if same_episode(&initial_journal, active) {
                    initial_journal.state
                } else {
                    "waiting_for_previous_reset".into()
                },
                initial_journal.reason,
                initial_journal.updated_at,
                true,
            ));
        }
        if !weekly_exhausted(active) {
            WeeklyResetStatusService::clear_completed_episode_if_restored(settings, Some(active))?;
            return Ok(report("ready", None, None, false));
        }
        if active.last_error.is_some() {
            return Ok(report(
                "waiting_for_fresh_quota",
                Some("active_account_usage_read_failed".into()),
                None,
                false,
            ));
        }
        if active.last_credits.unwrap_or(0) == 0 {
            return Ok(report("no_credit", None, None, false));
        }
        if !threshold_eligible(active, settings.auto_reset_weekly_min_remaining_seconds)? {
            return Ok(report("waiting_for_window", None, None, false));
        }

        // Hold the same operation lock as manual reset before creating a new
        // auto journal. Otherwise the two paths can each persist a pending
        // attempt and spend separate credits for the same account.
        let _operation = match recovery::operation_lock() {
            Ok(lock) => lock,
            Err(error) => {
                return Ok(report(
                    "waiting_for_other_automation",
                    Some(error),
                    None,
                    true,
                ))
            }
        };
        let mut journal = ResetJournalStore::load()?;
        let current = storage::load_accounts()?;
        let current_active = current
            .accounts
            .iter()
            .find(|account| account.id == active.id);
        let Some(current_active) = current_active else {
            return Ok(report(
                "policy_changed",
                Some("active_account_changed_before_reset".into()),
                journal.updated_at,
                false,
            ));
        };
        if current.active_account_id.as_deref() != Some(active.id.as_str())
            || current_active.account_id != active.account_id
        {
            return Ok(report(
                "policy_changed",
                Some("active_account_changed_before_reset".into()),
                journal.updated_at,
                false,
            ));
        }
        if crate::setup::unresolved_manual_reset()? {
            return Ok(report(
                "waiting_for_manual_reset",
                Some("manual_reset_attempt_unresolved".into()),
                journal.updated_at,
                true,
            ));
        }
        let journal_matches_episode = same_episode(&journal, active);
        if unresolved_attempt(&journal) && !journal_matches_episode {
            return Ok(report(
                "waiting_for_previous_reset",
                journal.reason,
                journal.updated_at,
                true,
            ));
        }
        let blocked_threads = environment.quota_blocked_threads();
        let Some(discovered_anchor) = blocked_threads.first().cloned() else {
            if journal_matches_episode {
                if journal.state == "applied" {
                    return Ok(report("applied", journal.reason, journal.updated_at, true));
                }
                if terminal_no_spend_state(&journal.state) {
                    return Ok(report(
                        journal.state,
                        journal.reason,
                        journal.updated_at,
                        false,
                    ));
                }
                let suppress_auto_switch = matches!(journal.state.as_str(), "pending" | "unknown");
                return Ok(report(
                    if suppress_auto_switch {
                        "waiting_for_original_task"
                    } else {
                        "waiting_for_task"
                    },
                    journal.reason,
                    journal.updated_at,
                    suppress_auto_switch,
                ));
            }
            return Ok(report("waiting_for_task", None, None, false));
        };

        if journal_matches_episode {
            if journal.state == "applied" {
                return Ok(report("applied", journal.reason, journal.updated_at, true));
            }
            if terminal_no_spend_state(&journal.state) {
                return Ok(report(
                    journal.state,
                    journal.reason,
                    journal.updated_at,
                    false,
                ));
            }
            let Some(original_anchor) = journal.thread_id.clone() else {
                return Ok(report(
                    "journal_error",
                    Some("auto_reset_journal_missing_anchor".into()),
                    journal.updated_at,
                    true,
                ));
            };
            if !blocked_threads.contains(&original_anchor) {
                let outcome_may_be_unknown =
                    matches!(journal.state.as_str(), "pending" | "unknown");
                return Ok(report(
                    "waiting_for_original_task",
                    Some("original_reset_task_is_no_longer_quota_blocked".into()),
                    journal.updated_at,
                    outcome_may_be_unknown,
                ));
            }
        } else {
            // A new attempt stays in memory until every final check passes;
            // the dispatch service persists it as `pending` only immediately
            // before sending the request.
            journal = ResetJournal {
                version: JOURNAL_VERSION,
                episode_key: Some(episode_key(active)),
                account_id: Some(active.account_id.clone()),
                thread_id: Some(discovered_anchor),
                idempotency_key: Some(new_idempotency_key()?),
                state: "ready".into(),
                reason: None,
                updated_at: None,
            };
        }
        ResetDispatchService::dispatch(
            settings,
            active,
            environment,
            journal,
            journal_matches_episode,
            &blocked_threads,
        )
    }
}

#[cfg(test)]
#[path = "weekly_reset_service.test.rs"]
mod tests;
