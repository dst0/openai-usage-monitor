//! Conservative weekly reset-credit automation.
//!
//! A reset credit is spent through the authenticated ChatGPT service used by
//! the quota reader. Desktop remains the sole owner of threads and is used
//! only for post-reset task recovery.

#[path = "auto_reset/auto_reset_report.rs"]
mod auto_reset_report;
#[path = "auto_reset/auto_reset_status.rs"]
mod auto_reset_status;
#[path = "auto_reset/reset_journal.rs"]
mod reset_journal;
#[path = "auto_reset/reset_journal_store.rs"]
mod reset_journal_store;
#[path = "auto_reset/reset_outcome_service.rs"]
mod reset_outcome_service;
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
    weekly_reset_service::WeeklyResetService::maybe_consume_weekly_reset(settings, active)
}

#[cfg(test)]
use reset_journal::ResetJournal;
#[cfg(test)]
use weekly_reset_policy::{terminal_no_spend_state, threshold_eligible, weekly_exhausted};
#[cfg(test)]
fn load_journal_at(path: &std::path::Path) -> Result<ResetJournal, String> {
    reset_journal_store::ResetJournalStore::load_at(path)
}
#[cfg(test)]
fn write_journal_at(path: &std::path::Path, journal: &ResetJournal) -> Result<(), String> {
    reset_journal_store::ResetJournalStore::write_at(path, journal)
}

#[cfg(test)]
#[path = "auto_reset.test.rs"]
mod tests;
