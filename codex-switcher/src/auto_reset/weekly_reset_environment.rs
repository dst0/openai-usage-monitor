use crate::models::AccountConfig;
use crate::quota::ResetCreditConsumeOutcome;

/// Host effects used by one automatic reset decision. Tests substitute a fake
/// so dispatch ordering is proven without a live thread database, Desktop
/// process table, or reset-credit service.
pub(super) trait WeeklyResetEnvironment {
    /// Recent unarchived user tasks whose latest turn stopped on quota.
    fn quota_blocked_threads(&self) -> Vec<String>;

    /// Whether the official Desktop app is running. An inspection error must
    /// be returned rather than treated as either answer.
    fn desktop_running(&self) -> Result<bool, String>;

    /// Sends one reset-credit request with the already persisted key.
    fn consume_reset_credit(
        &self,
        account: &AccountConfig,
        idempotency_key: &str,
    ) -> ResetCreditConsumeOutcome;
}
