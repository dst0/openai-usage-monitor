//! Conservative weekly reset-credit automation.
//!
//! A reset credit is spent through the authenticated ChatGPT service used by
//! the quota reader. Desktop remains the sole owner of threads and is used
//! only for post-reset task recovery.

#[path = "auto_reset/applied_reset_settlement_service.rs"]
mod applied_reset_settlement_service;
#[path = "auto_reset/auto_reset_report.rs"]
mod auto_reset_report;
#[path = "auto_reset/auto_reset_status.rs"]
mod auto_reset_status;
#[path = "auto_reset/reset_dispatch_service.rs"]
mod reset_dispatch_service;
#[path = "auto_reset/reset_journal.rs"]
mod reset_journal;
#[path = "auto_reset/reset_journal_store.rs"]
mod reset_journal_store;
#[path = "auto_reset/reset_outcome_service.rs"]
mod reset_outcome_service;
#[path = "auto_reset/reset_pending_mark_service.rs"]
mod reset_pending_mark_service;
#[path = "auto_reset/reset_preflight.rs"]
mod reset_preflight;
#[path = "auto_reset/reset_preflight_service.rs"]
mod reset_preflight_service;
#[path = "auto_reset/system_weekly_reset_environment.rs"]
mod system_weekly_reset_environment;
#[path = "auto_reset/weekly_reset_environment.rs"]
mod weekly_reset_environment;
#[path = "auto_reset/weekly_reset_policy.rs"]
mod weekly_reset_policy;
#[path = "auto_reset/weekly_reset_service.rs"]
mod weekly_reset_service;
#[path = "auto_reset/weekly_reset_status_service.rs"]
mod weekly_reset_status_service;

pub(crate) use auto_reset_report::AutoResetReport;
pub(crate) use auto_reset_status::AutoResetStatus;
pub(crate) use weekly_reset_policy::new_idempotency_key;

use crate::models::{AccountConfig, Settings};

pub(crate) fn status_for_active(
    settings: &Settings,
    active: Option<&AccountConfig>,
) -> AutoResetStatus {
    weekly_reset_status_service::WeeklyResetStatusService::status_for_active(settings, active)
}

pub(crate) fn maybe_consume_weekly_reset(
    settings: &Settings,
    active: &AccountConfig,
) -> Result<AutoResetReport, String> {
    weekly_reset_service::WeeklyResetService::maybe_consume_weekly_reset(
        settings,
        active,
        &system_weekly_reset_environment::SystemWeeklyResetEnvironment,
    )
}

/// Prevent a manual reset from minting a second request while an automatic
/// request for this account route still has an uncertain outcome. A changed
/// cached window marker is not settlement evidence.
pub(crate) fn unresolved_auto_reset_for(account: &AccountConfig) -> Result<bool, String> {
    let journal =
        reset_journal_store::ResetJournalStore::new(&crate::state_file::SystemStateFileOperations)
            .load()?;
    Ok(weekly_reset_policy::unresolved_for_route(&journal, account))
}

#[cfg(test)]
#[path = "auto_reset/fake_weekly_reset_environment.test.rs"]
mod fake_weekly_reset_environment;
#[cfg(test)]
use reset_journal::ResetJournal;
#[cfg(test)]
use weekly_reset_policy::{
    remaining_exceeds_threshold, terminal_no_spend_state, threshold_eligible, weekly_exhausted,
};
#[cfg(test)]
fn journal_store_for(path: &std::path::Path) -> reset_journal_store::ResetJournalStore<'static> {
    // Test journals always use the production file name inside a private
    // temporary directory.
    assert_eq!(path.file_name().unwrap(), "auto-reset-state.json");
    reset_journal_store::ResetJournalStore::in_directory(
        path.parent().unwrap().to_path_buf(),
        &crate::state_file::SystemStateFileOperations,
    )
}
#[cfg(test)]
fn load_journal_at(path: &std::path::Path) -> Result<ResetJournal, String> {
    journal_store_for(path).load()
}
#[cfg(test)]
fn write_journal_at(path: &std::path::Path, journal: &ResetJournal) -> Result<(), String> {
    journal_store_for(path).write(journal)
}

#[cfg(test)]
#[path = "auto_reset.test.rs"]
mod tests;
