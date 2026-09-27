use super::weekly_reset_environment::WeeklyResetEnvironment;
use crate::models::{AccountConfig, WhamUsageResponse};
use crate::quota::{self, ResetCreditConsumeOutcome};
use crate::state_file::{StateFileOperations, SystemStateFileOperations};
use crate::{recovery, switcher};

/// Production host effects: the Desktop thread index, the process table, the
/// authenticated ChatGPT reset-credit and usage endpoints, Desktop recovery,
/// and the real filesystem.
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
        #[cfg(test)]
        crate::test_live_system::forbid("reset-credit service");
        quota::consume_rate_limit_reset_credit(account, idempotency_key)
    }

    fn read_usage(&self, account: &AccountConfig) -> Result<WhamUsageResponse, String> {
        #[cfg(test)]
        crate::test_live_system::forbid("usage service");
        quota::fetch_account_usage_read_only(&mut account.clone())
    }

    fn recover_threads(&self, thread_ids: &[String]) -> Result<(), String> {
        #[cfg(test)]
        crate::test_live_system::forbid("Desktop task recovery");
        recovery::recover_threads(thread_ids, recovery::RecoveryMode::DiscoveredOnly)
    }

    fn journal_files(&self) -> &dyn StateFileOperations {
        &SystemStateFileOperations
    }
}

#[cfg(test)]
#[path = "system_weekly_reset_environment.test.rs"]
mod tests;
