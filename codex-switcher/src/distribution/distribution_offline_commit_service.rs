use super::app_lifecycle::AppLifecycle;
use super::desktop_app_session::DesktopAppSession;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_journal::DistributionJournal;
use super::distribution_offline_registry_service::DistributionOfflineRegistryService;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::distribution_shared_auth_guard::DistributionSharedAuthGuard;
use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountsFile, AuthJson};
use std::path::Path;

pub(super) struct DistributionOfflineCommitService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
}

impl<'a> DistributionOfflineCommitService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
    ) -> Self {
        Self { lifecycle, logger }
    }

    pub(super) fn run(
        &self,
        home: &Path,
        journal: &mut DistributionJournal,
        plan: &DistributionPlan,
        request: &DistributionRequest,
        accounts: &mut AccountsFile,
        operation_id: &str,
    ) -> Result<(), String> {
        self.run_with_marker_writer(
            home,
            journal,
            plan,
            request,
            accounts,
            operation_id,
            DesktopAppSession::save,
        )
    }

    fn run_with_marker_writer(
        &self,
        home: &Path,
        journal: &mut DistributionJournal,
        plan: &DistributionPlan,
        request: &DistributionRequest,
        accounts: &mut AccountsFile,
        operation_id: &str,
        save_marker: impl FnOnce(&DesktopAppSession, &Path) -> Result<(), String>,
    ) -> Result<(), String> {
        let before_accounts = accounts.clone();
        let marker_path = home.join("desktop-app-session.json");
        let marker_before = if plan.app_switch_needed {
            DesktopAppSession::load_checked(&marker_path)?
        } else {
            None
        };
        let mut auth_commit: Option<(AuthJson, AuthJson)> = None;
        if plan.cli_switch_needed {
            let target_id = plan
                .target_cli_id
                .as_deref()
                .ok_or("Target CLI account is unavailable")?;
            DistributionSharedAuthGuard::require_desktop_stopped(self.lifecycle)?;
            journal.update_phase(home, "auth_commit_cli")?;
            let target = DistributionAccountCommitService::find_account(accounts, target_id)?;
            let (previous, committed) =
                DistributionAccountCommitService::apply_auth_tokens(self.lifecycle, &target)?;
            let committed_registry = DistributionOfflineRegistryService::commit(
                &target,
                before_accounts.active_account_id.as_deref(),
            );
            *accounts = match committed_registry {
                Ok(latest) => latest,
                Err(error) => {
                    let rollback = DistributionAccountCommitService::restore_when_desktop_stopped(
                        self.lifecycle,
                        &previous,
                        &committed,
                    );
                    return Err(match rollback {
                        Ok(()) => format!(
                            "CLI authentication rolled back after registry commit failure: {error}; inspect retained distribution journal"
                        ),
                        Err(rollback) => format!(
                            "CLI registry commit failed: {error}; authentication rollback unverified: {rollback}"
                        ),
                    });
                }
            };
            self.logger.log_action(
                operation_id,
                "AUTH_COMMIT_CLI",
                request.trigger.as_str(),
                &request.reason,
                &format!(
                    "CLI switched account_ref={}",
                    LogRedactionService::sanitize_field("account_id", target_id)
                ),
            );
            auth_commit = Some((previous, committed));
        }

        if plan.app_switch_needed {
            let target_id = plan
                .target_app_id
                .as_deref()
                .ok_or("Target Desktop account is unavailable")?;
            let marker = DesktopAppSession::new(target_id);
            if let Err(error) = save_marker(&marker, &marker_path) {
                return Err(self.rollback_marker_failure(
                    home,
                    accounts,
                    &before_accounts,
                    auth_commit.as_ref(),
                    marker_before.as_ref(),
                    &marker,
                    error,
                ));
            }
            self.logger.log_action(
                operation_id,
                "DESKTOP_SESSION",
                request.trigger.as_str(),
                &request.reason,
                &format!(
                    "Desktop session account_ref={}",
                    LogRedactionService::sanitize_field("account_id", target_id)
                ),
            );
        }
        if let Some((_, committed)) = auth_commit.as_ref() {
            // Registry and marker writes can take long enough for Desktop to
            // start or refresh the shared auth after the first readback.
            DistributionAccountCommitService::verify_offline_auth(self.lifecycle, committed)?;
        }
        Ok(())
    }

    fn rollback_marker_failure(
        &self,
        home: &Path,
        accounts: &mut AccountsFile,
        before_accounts: &AccountsFile,
        auth_commit: Option<&(AuthJson, AuthJson)>,
        marker_before: Option<&DesktopAppSession>,
        attempted_marker: &DesktopAppSession,
        error: String,
    ) -> String {
        if let Some((previous, committed)) = auth_commit {
            let Some(target_id) = accounts.active_account_id.as_deref() else {
                return format!("Desktop marker save failed: {error}; target registry identity unavailable for rollback");
            };
            if let Err(binding) = DistributionOfflineRegistryService::verify_rollback_binding(
                target_id,
                before_accounts,
                previous,
            ) {
                return format!(
                    "Desktop marker save failed: {error}; prior account binding changed; rollback unverified: {binding}"
                );
            }
            if let Err(rollback) = DistributionAccountCommitService::restore_when_desktop_stopped(
                self.lifecycle,
                previous,
                committed,
            ) {
                return format!(
                    "Desktop marker save failed: {error}; authentication rollback unverified: {rollback}"
                );
            }
            match DistributionOfflineRegistryService::rollback(target_id, before_accounts, previous)
            {
                Ok(restored) => *accounts = restored,
                Err(rollback) => {
                    return format!(
                        "Desktop marker save failed: {error}; registry rollback unverified: {rollback}"
                    );
                }
            }
            if let Err(verification) =
                DistributionAccountCommitService::verify_offline_auth(self.lifecycle, previous)
                    .and_then(|_| {
                        DistributionOfflineRegistryService::verify_restored(
                            before_accounts,
                            previous,
                        )
                    })
            {
                return format!(
                    "Desktop marker save failed: {error}; rollback readback unverified: {verification}"
                );
            }
        }
        if let Err(restore) = DesktopAppSession::restore_after_failed_save(
            &home.join("desktop-app-session.json"),
            marker_before,
            attempted_marker,
        ) {
            return format!(
                "Desktop marker save failed: {error}; marker rollback unverified: {restore}"
            );
        }
        match DistributionJournal::clear(home) {
            Ok(()) => format!("Desktop marker save failed; account switch rolled back: {error}"),
            Err(clear) => {
                format!("Desktop marker save failed: {error}; journal cleanup failed: {clear}")
            }
        }
    }
}

#[cfg(test)]
#[path = "distribution_offline_commit_service.test.rs"]
mod tests;
