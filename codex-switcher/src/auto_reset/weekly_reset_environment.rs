use crate::models::{AccountConfig, WhamUsageResponse};
use crate::quota::ResetCreditConsumeOutcome;
use crate::state_file::StateFileOperations;

/// Host effects used by one automatic reset decision. Tests substitute a fake
/// so dispatch ordering, the applied hand-off, and journal durability are
/// proven without a live thread database, Desktop process table, reset-credit
/// or usage service, or a filesystem that cannot be made to fail.
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

    /// Reads fresh usage for `account` after an applied reset, without
    /// refreshing or persisting its tokens.
    fn read_usage(&self, account: &AccountConfig) -> Result<WhamUsageResponse, String>;

    /// Hands the blocked tasks to Desktop's owner-routed recovery for
    /// discovered targets.
    fn recover_threads(&self, thread_ids: &[String]) -> Result<(), String>;

    /// Filesystem calls for the automatic reset journal.
    fn journal_files(&self) -> &dyn StateFileOperations;
}
