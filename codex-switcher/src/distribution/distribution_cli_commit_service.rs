use super::app_lifecycle::AppLifecycle;
use super::desktop_session_verification_service::DesktopSessionVerificationService;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_journal::DistributionJournal;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountsFile, AuthJson};
use crate::storage;
use std::path::Path;

pub(super) struct DistributionCliCommitService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
    home: &'a Path,
}

impl<'a> DistributionCliCommitService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
        home: &'a Path,
    ) -> Self {
        Self {
            lifecycle,
            logger,
            home,
        }
    }

    pub(super) fn after_relaunch(
        &self,
        accounts: &mut AccountsFile,
        plan: &DistributionPlan,
        original_auth: &AuthJson,
        journal: &mut DistributionJournal,
        op_id: &str,
        request: &DistributionRequest,
    ) -> Result<Option<String>, String> {
        let Some(target_id) = plan.target_cli_id.as_deref() else {
            return Ok(None);
        };
        if let Err(error) = journal.update_phase(self.home, "auth_commit_cli") {
            let rollback = self.restore_original_cli(original_auth, plan.current_cli_id.as_deref());
            return Ok(Some(Self::with_rollback_error(error, rollback)));
        }
        let target = match DistributionAccountCommitService::find_account(accounts, target_id) {
            Ok(target) => target,
            Err(error) => {
                let rollback =
                    self.restore_original_cli(original_auth, plan.current_cli_id.as_deref());
                return Ok(Some(Self::with_rollback_error(error, rollback)));
            }
        };
        match DistributionAccountCommitService::commit_cli_account(accounts, &target) {
            Ok(()) => {
                self.logger.log_action(
                    op_id,
                    "AUTH_COMMIT_CLI",
                    request.trigger.as_str(),
                    &request.reason,
                    &format!(
                        "Target CLI account_ref={} no_second_restart=true",
                        LogRedactionService::sanitize_field("account_id", target_id)
                    ),
                );
                Ok(
                    DesktopSessionVerificationService::new(self.lifecycle, self.home)
                        .reconcile_cli_binding(target_id)
                        .err(),
                )
            }
            Err(error) => {
                let rollback =
                    self.restore_original_cli(original_auth, plan.current_cli_id.as_deref());
                Ok(Some(Self::with_rollback_error(error, rollback)))
            }
        }
    }

    pub(super) fn without_restart(
        &self,
        accounts: &mut AccountsFile,
        plan: &DistributionPlan,
        journal: &mut DistributionJournal,
        op_id: &str,
        request: &DistributionRequest,
    ) -> Result<Option<String>, String> {
        if !plan.cli_switch_needed {
            return Ok(None);
        }
        let Some(target_id) = plan.target_cli_id.as_deref() else {
            return Ok(None);
        };
        journal.update_phase(self.home, "auth_commit_cli")?;
        let target = DistributionAccountCommitService::find_account(accounts, target_id)?;
        if let Err(error) = DistributionAccountCommitService::commit_cli_account(accounts, &target)
        {
            return Ok(Some(error));
        }
        self.logger.log_action(
            op_id,
            "AUTH_COMMIT_CLI",
            request.trigger.as_str(),
            &request.reason,
            &format!(
                "CLI switched account_ref={}",
                LogRedactionService::sanitize_field("account_id", target_id)
            ),
        );
        if plan.current_app_id.is_some() {
            Ok(
                DesktopSessionVerificationService::new(self.lifecycle, self.home)
                    .update_cli_binding(plan.current_cli_id.as_deref().unwrap_or(""), target_id)
                    .err(),
            )
        } else {
            Ok(None)
        }
    }

    fn restore_original_cli(
        &self,
        original_auth: &AuthJson,
        original_cli_id: Option<&str>,
    ) -> Result<(), String> {
        storage::write_active_auth_json(original_auth)?;
        if let Some(cli_id) = original_cli_id {
            DesktopSessionVerificationService::new(self.lifecycle, self.home)
                .reconcile_cli_binding(cli_id)?;
        }
        Ok(())
    }

    fn with_rollback_error(error: String, rollback: Result<(), String>) -> String {
        match rollback {
            Ok(()) => error,
            Err(restore) => format!("{error}; CLI rollback failed: {restore}"),
        }
    }
}
