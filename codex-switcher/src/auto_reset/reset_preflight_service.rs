use super::reset_preflight::ResetPreflight;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use super::weekly_reset_policy::{episode_key, threshold_eligible, weekly_exhausted};
use crate::models::{AccountConfig, Settings};
use crate::{quota, storage};

/// Final eligibility checks for one automatic reset. They run under the
/// recovery operation lock and before the attempt is persisted, send nothing,
/// and never write the reset journal (loading the registry may still persist
/// its usual duplicate-account repair), so a refusal cannot leave an unsent
/// `pending` attempt behind.
pub(super) struct ResetPreflightService;

impl ResetPreflightService {
    pub(super) fn check(
        settings: &Settings,
        active: &AccountConfig,
        idempotency_key: &str,
        environment: &impl WeeklyResetEnvironment,
    ) -> Result<ResetPreflight, String> {
        // A menu change or account switch while quota or tasks were being read
        // must never spend a credit for an outdated account or policy. Loading
        // repairs an active ID that names no account, so a missing account is
        // reported as the same policy change.
        let latest = storage::load_accounts()?;
        let still_active = latest.active_account_id.as_deref() == Some(active.id.as_str());
        let still_enabled = latest.settings.auto_reset_weekly_enabled
            && latest.settings.auto_reset_weekly_min_remaining_seconds
                == settings.auto_reset_weekly_min_remaining_seconds;
        let latest_active = latest
            .accounts
            .iter()
            .find(|account| account.id == active.id)
            .filter(|_| still_active && still_enabled);
        let Some(latest_active) = latest_active else {
            return Ok(ResetPreflight::refused(
                "policy_changed",
                Some("active_account_or_auto_reset_policy_changed"),
            ));
        };
        // The freshly persisted service snapshot, not the daemon's earlier
        // copy, decides whether the pool is still exhausted on this route.
        if latest_active.account_id != active.account_id
            || latest_active.last_error.is_some()
            || !weekly_exhausted(latest_active)
        {
            return Ok(ResetPreflight::refused(
                "policy_changed",
                Some("active_account_or_weekly_quota_changed"),
            ));
        }
        // A new attempt is keyed to the daemon snapshot's weekly window; a
        // different registry window must not be spent under that key.
        if episode_key(latest_active) != episode_key(active) {
            return Ok(ResetPreflight::refused(
                "policy_changed",
                Some("weekly_reset_window_changed"),
            ));
        }
        if latest_active.last_credits.unwrap_or(0) == 0 {
            return Ok(ResetPreflight::refused("no_credit", None));
        }
        if !threshold_eligible(
            latest_active,
            latest.settings.auto_reset_weekly_min_remaining_seconds,
        )? {
            return Ok(ResetPreflight::refused("waiting_for_window", None));
        }
        let live_auth_matches = storage::read_active_auth_json()
            .ok()
            .and_then(|auth| auth.tokens)
            .is_some_and(|tokens| {
                tokens.access_token == latest_active.tokens.access_token
                    && tokens.account_id.as_deref() == latest_active.tokens.account_id.as_deref()
            });
        if !live_auth_matches {
            return Ok(ResetPreflight::refused(
                "policy_changed",
                Some("active_auth_changed_before_reset"),
            ));
        }
        // A request that cannot be built (route mismatch, unusable token or
        // key) would never leave; refuse it here instead of after `pending`.
        if let Err(reason) = quota::reset_request_blocker(latest_active, idempotency_key) {
            return Ok(ResetPreflight::refused("policy_changed", Some(&reason)));
        }
        if !environment.desktop_running()? {
            return Ok(ResetPreflight::Refused {
                state: "waiting_for_desktop".into(),
                reason: Some("desktop_not_running".into()),
                record: true,
            });
        }
        Ok(ResetPreflight::Ready(Box::new(latest_active.clone())))
    }
}
