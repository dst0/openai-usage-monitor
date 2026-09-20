use super::reset_journal::{ResetJournal, JOURNAL_VERSION};
use super::reset_journal_store::ResetJournalStore;
use super::reset_outcome_service::ResetOutcomeService;
use super::weekly_reset_policy::{
    episode_key, new_idempotency_key, now_string, report, same_episode, terminal_no_spend_state,
    threshold_eligible, weekly_exhausted,
};
use super::weekly_reset_status_service::WeeklyResetStatusService;
use super::AutoResetReport;
use crate::models::{AccountConfig, Settings};
use crate::{quota, recovery, storage, switcher};

pub(super) struct WeeklyResetService;

impl WeeklyResetService {
    /// Applies the account-bound reset policy once. The daemon calls this only
    /// after obtaining fresh quota data; any result which could have consumed a
    /// credit suppresses account rotation until the next fresh usage snapshot.
    pub(super) fn maybe_consume_weekly_reset(
        settings: &Settings,
        active: &AccountConfig,
    ) -> Result<AutoResetReport, String> {
        if !settings.auto_reset_weekly_enabled {
            return Ok(report("disabled", None, None, false));
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

        let mut journal = ResetJournalStore::load()?;
        let journal_matches_episode = same_episode(&journal, active);
        let blocked_threads = switcher::detect_recent_quota_blocked_user_threads();
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
            let anchor_thread = discovered_anchor;
            journal = ResetJournal {
                version: JOURNAL_VERSION,
                episode_key: Some(episode_key(active)),
                account_id: Some(active.account_id.clone()),
                thread_id: Some(anchor_thread.clone()),
                idempotency_key: Some(new_idempotency_key()?),
                state: "pending".into(),
                reason: None,
                updated_at: Some(now_string()),
            };
            // Persist before dispatch. A process crash after the service request
            // is sent must retry this exact operation rather than manufacture a
            // new credit use.
            ResetJournalStore::write(&journal)?;
        }

        let idempotency_key = journal
            .idempotency_key
            .clone()
            .ok_or("Auto-reset journal has no idempotency key")?;
        let _operation = match recovery::operation_lock() {
            Ok(lock) => lock,
            Err(error) => {
                return Ok(report(
                    "waiting_for_other_automation",
                    Some(error),
                    journal.updated_at,
                    true,
                ))
            }
        };

        // Re-check both the active account and opt-in after acquiring the operation
        // lock. A menu change or account switch while quota was being fetched must
        // never spend a credit for an outdated account.
        let latest = storage::load_accounts()?;
        let still_active = latest.active_account_id.as_deref() == Some(active.id.as_str());
        let still_enabled = latest.settings.auto_reset_weekly_enabled
            && latest.settings.auto_reset_weekly_min_remaining_seconds
                == settings.auto_reset_weekly_min_remaining_seconds;
        if !still_active || !still_enabled {
            return Ok(report(
                "policy_changed",
                Some("active_account_or_auto_reset_policy_changed".into()),
                journal.updated_at,
                false,
            ));
        }
        let Some(latest_active) = latest
            .accounts
            .iter()
            .find(|account| account.id == active.id)
        else {
            return Ok(report(
                "policy_changed",
                Some("active_account_disappeared".into()),
                journal.updated_at,
                false,
            ));
        };
        // Recheck the freshly persisted service snapshot after taking the same
        // operation lock used by account switching. This prevents a stale daemon
        // decision from spending against a recovered or differently-routed account.
        if latest_active.account_id != active.account_id
            || latest_active.last_error.is_some()
            || !weekly_exhausted(latest_active)
        {
            return Ok(report(
                "policy_changed",
                Some("active_account_or_weekly_quota_changed".into()),
                journal.updated_at,
                false,
            ));
        }
        if latest_active.last_credits.unwrap_or(0) == 0 {
            return Ok(report("no_credit", None, journal.updated_at, false));
        }
        if !threshold_eligible(
            latest_active,
            latest.settings.auto_reset_weekly_min_remaining_seconds,
        )? {
            return Ok(report(
                "waiting_for_window",
                None,
                journal.updated_at,
                false,
            ));
        }
        let live_auth_matches = storage::read_active_auth_json()
            .ok()
            .and_then(|auth| auth.tokens)
            .is_some_and(|tokens| {
                tokens.access_token == latest_active.tokens.access_token
                    && tokens.account_id.as_deref() == latest_active.tokens.account_id.as_deref()
            });
        if !live_auth_matches {
            return Ok(report(
                "policy_changed",
                Some("active_auth_changed_before_reset".into()),
                journal.updated_at,
                false,
            ));
        }
        if !switcher::is_codex_app_running() {
            journal.state = "waiting_for_desktop".into();
            journal.reason = Some("desktop_not_running".into());
            journal.updated_at = Some(now_string());
            ResetJournalStore::write(&journal)?;
            return Ok(report(
                journal.state,
                journal.reason,
                journal.updated_at,
                false,
            ));
        }

        let outcome = quota::consume_rate_limit_reset_credit(latest_active, &idempotency_key);
        ResetOutcomeService::handle(outcome, journal, &blocked_threads, latest_active)
    }
}
