use super::reset_journal::ResetJournal;
use super::reset_journal_store::ResetJournalStore;
use super::weekly_reset_policy::{
    same_episode, threshold_eligible, unresolved_attempt, weekly_exhausted,
};
use super::AutoResetStatus;
use crate::models::{AccountConfig, Settings};

pub(super) struct WeeklyResetStatusService;

impl WeeklyResetStatusService {
    /// Gives the menu a durable, safe-to-display state before the daemon decides
    /// whether an eligible task exists. It never performs an IPC request.
    pub(super) fn status_for_active(
        settings: &Settings,
        active: Option<&AccountConfig>,
    ) -> AutoResetStatus {
        if !settings.auto_reset_weekly_enabled {
            return AutoResetStatus {
                state: "disabled".into(),
                reason: None,
                last_event_at: None,
            };
        }
        let Some(active) = active else {
            return AutoResetStatus {
                state: "waiting_for_active_account".into(),
                reason: None,
                last_event_at: None,
            };
        };
        let journal = match ResetJournalStore::load() {
            Ok(journal) => journal,
            Err(_) => {
                return AutoResetStatus {
                    state: "journal_error".into(),
                    reason: Some("auto_reset_journal_invalid".into()),
                    last_event_at: None,
                }
            }
        };
        if unresolved_attempt(&journal) {
            return AutoResetStatus {
                state: if same_episode(&journal, active) {
                    journal.state
                } else {
                    "waiting_for_previous_reset".into()
                },
                reason: journal.reason,
                last_event_at: journal.updated_at,
            };
        }
        if !weekly_exhausted(active) {
            return AutoResetStatus {
                state: "ready".into(),
                reason: None,
                last_event_at: None,
            };
        }
        if active.last_error.is_some() {
            return AutoResetStatus {
                state: "waiting_for_fresh_quota".into(),
                reason: Some("active_account_usage_read_failed".into()),
                last_event_at: None,
            };
        }
        if active.last_credits.unwrap_or(0) == 0 {
            return AutoResetStatus {
                state: "no_credit".into(),
                reason: None,
                last_event_at: None,
            };
        }
        if !threshold_eligible(active, settings.auto_reset_weekly_min_remaining_seconds)
            .unwrap_or(false)
        {
            return AutoResetStatus {
                state: "waiting_for_window".into(),
                reason: None,
                last_event_at: None,
            };
        }
        match journal {
            journal if same_episode(&journal, active) => AutoResetStatus {
                state: journal.state,
                reason: journal.reason,
                last_event_at: journal.updated_at,
            },
            _ => AutoResetStatus {
                state: "waiting_for_task".into(),
                reason: None,
                last_event_at: None,
            },
        }
    }

    /// Clears a terminal previous episode once usage shows a non-zero weekly
    /// pool. Pending and unknown requests remain until reconciliation.
    pub(super) fn clear_completed_episode_if_restored(
        settings: &Settings,
        active: Option<&AccountConfig>,
    ) -> Result<(), String> {
        if !settings.auto_reset_weekly_enabled || active.is_none_or(weekly_exhausted) {
            return Ok(());
        }
        let _operation = crate::recovery::operation_lock()?;
        let journal = ResetJournalStore::load()?;
        // A restored pool can be the delayed effect of the very request whose
        // response was lost. Keep its key until explicit reconciliation; a
        // manual reset must still see the unresolved automatic attempt.
        if journal.episode_key.is_some() && !matches!(journal.state.as_str(), "pending" | "unknown")
        {
            ResetJournalStore::write(&ResetJournal::default())?;
        }
        Ok(())
    }
}
