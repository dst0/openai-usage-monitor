use super::reset_preflight::ResetPreflight;
use super::weekly_reset_environment::WeeklyResetEnvironment;
use super::weekly_reset_policy::{threshold_eligible, weekly_exhausted};
use crate::models::{AccountConfig, Settings};
use crate::storage;

/// Final, side-effect-free eligibility checks for one automatic reset. They
/// run under the recovery operation lock and before the attempt is persisted,
/// so a refusal can never leave an unsent `pending` attempt behind.
pub(super) struct ResetPreflightService;

impl ResetPreflightService {
    pub(super) fn check(
        settings: &Settings,
        active: &AccountConfig,
        environment: &impl WeeklyResetEnvironment,
    ) -> Result<ResetPreflight, String> {
        // A menu change or account switch while quota or tasks were being read
        // must never spend a credit for an outdated account or policy.
        let latest = storage::load_accounts()?;
        let still_active = latest.active_account_id.as_deref() == Some(active.id.as_str());
        let still_enabled = latest.settings.auto_reset_weekly_enabled
            && latest.settings.auto_reset_weekly_min_remaining_seconds
                == settings.auto_reset_weekly_min_remaining_seconds;
        if !still_active || !still_enabled {
            return Ok(ResetPreflight::refused(
                "policy_changed",
                Some("active_account_or_auto_reset_policy_changed"),
            ));
        }
        let Some(latest_active) = latest
            .accounts
            .iter()
            .find(|account| account.id == active.id)
        else {
            return Ok(ResetPreflight::refused(
                "policy_changed",
                Some("active_account_disappeared"),
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
