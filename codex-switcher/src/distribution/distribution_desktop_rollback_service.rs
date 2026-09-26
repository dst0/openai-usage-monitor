use super::app_lifecycle::AppLifecycle;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_journal::DistributionJournal;
use super::distribution_plan::DistributionPlan;
use crate::models::{AccountsFile, AuthJson};
use std::path::Path;

pub(super) struct DistributionDesktopRollbackService<'a> {
    lifecycle: &'a dyn AppLifecycle,
}

impl<'a> DistributionDesktopRollbackService<'a> {
    pub(super) fn new(lifecycle: &'a dyn AppLifecycle) -> Self {
        Self { lifecycle }
    }

    pub(super) fn before_auth_commit(
        &self,
        home: &Path,
        accounts: &AccountsFile,
        plan: &DistributionPlan,
        error: String,
        retain_journal: bool,
    ) -> String {
        self.lifecycle.abort_recovery();
        let relaunch = plan
            .current_app_id
            .as_deref()
            .ok_or_else(|| "Previous Desktop identity unavailable".to_string())
            .and_then(|id| {
                DistributionAccountCommitService::relaunch_if_auth_identity_matches(
                    self.lifecycle,
                    home,
                    accounts,
                    id,
                )
            });
        match relaunch {
            Ok(()) if retain_journal => format!(
                "Desktop switch aborted: {error}; previous Desktop relaunched; registry handoff remains pending"
            ),
            Ok(()) => match DistributionJournal::clear(home) {
                Ok(()) => format!("Desktop switch aborted: {error}; previous Desktop relaunched"),
                Err(clear) => {
                    format!("Desktop switch aborted: {error}; journal cleanup failed: {clear}")
                }
            },
            Err(relaunch) => format!(
                "Desktop switch aborted: {error}; previous Desktop relaunch unverified: {relaunch}"
            ),
        }
    }

    pub(super) fn after_auth_commit(
        &self,
        home: &Path,
        accounts: &AccountsFile,
        expected_previous_id: &str,
        previous: &AuthJson,
        committed: &AuthJson,
        error: String,
    ) -> String {
        self.lifecycle.abort_recovery();
        let rollback = DistributionAccountCommitService::rollback_and_relaunch_previous(
            self.lifecycle,
            home,
            accounts,
            expected_previous_id,
            previous,
            committed,
        );
        match rollback {
            Ok(()) => match DistributionJournal::clear(home) {
                Ok(()) => format!("Desktop switch rolled back: {error}"),
                Err(clear) => {
                    format!("Desktop switch rolled back: {error}; journal cleanup failed: {clear}")
                }
            },
            Err(rollback) => {
                format!("Desktop switch incomplete: {error}; rollback unverified: {rollback}")
            }
        }
    }
}
