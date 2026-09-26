use super::weekly_reset_environment::WeeklyResetEnvironment;
use crate::models::AccountConfig;
use crate::quota::{self, ResetCreditConsumeOutcome};
use crate::switcher;

/// Production host effects: the Desktop thread index, the process table, and
/// the authenticated ChatGPT reset-credit endpoint.
pub(super) struct SystemWeeklyResetEnvironment;

impl WeeklyResetEnvironment for SystemWeeklyResetEnvironment {
    fn quota_blocked_threads(&self) -> Vec<String> {
        switcher::detect_recent_quota_blocked_user_threads()
    }

    fn desktop_running(&self) -> Result<bool, String> {
        switcher::is_codex_app_running_checked()
    }

    fn consume_reset_credit(
        &self,
        account: &AccountConfig,
        idempotency_key: &str,
    ) -> ResetCreditConsumeOutcome {
        quota::consume_rate_limit_reset_credit(account, idempotency_key)
    }
}
