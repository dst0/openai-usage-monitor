use super::app_lifecycle::AppLifecycle;
use super::desktop_session_verification_service::DesktopSessionVerificationService;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_desktop_auth_handoff_service::DistributionDesktopAuthHandoffService;
use super::distribution_desktop_switch_service::DistributionDesktopSwitchService;
use super::distribution_journal::DistributionJournal;
use super::distribution_offline_commit_service::DistributionOfflineCommitService;
use super::distribution_outcome::{DistributionOutcome, DistributionStatus};
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::distribution_shared_auth_guard::DistributionSharedAuthGuard;
use super::distribution_state_preflight_service::DistributionStatePreflightService;
use super::system_app_lifecycle::SystemAppLifecycle;
use crate::models::AccountsFile;
use crate::recovery;
use crate::storage;
use std::fs::File;
use std::sync::Arc;

pub struct DistributionTransactionService {
    logger: DistributionAuditLogger,
    lifecycle: Arc<dyn AppLifecycle>,
}

impl Default for DistributionTransactionService {
    fn default() -> Self {
        Self::new(DistributionAuditLogger::default())
    }
}

impl DistributionTransactionService {
    pub fn new(logger: DistributionAuditLogger) -> Self {
        Self::with_lifecycle(logger, Arc::new(SystemAppLifecycle::default()))
    }

    pub fn with_lifecycle(
        logger: DistributionAuditLogger,
        lifecycle: Arc<dyn AppLifecycle>,
    ) -> Self {
        Self { logger, lifecycle }
    }

    pub fn execute(
        &self,
        op_id: &str,
        plan: &DistributionPlan,
        request: &DistributionRequest,
        accounts_file: AccountsFile,
    ) -> Result<DistributionOutcome, String> {
        let operation_lock = recovery::operation_lock()?;
        self.execute_locked(op_id, plan, request, accounts_file, &operation_lock)
    }

    pub(super) fn execute_locked(
        &self,
        op_id: &str,
        plan: &DistributionPlan,
        request: &DistributionRequest,
        mut accounts_file: AccountsFile,
        _operation_lock: &File,
    ) -> Result<DistributionOutcome, String> {
        let home = storage::codex_home();
        let trigger_str = request.trigger.as_str();
        self.logger.log_lock(
            op_id,
            trigger_str,
            &request.reason,
            "Acquired operation lock",
        );

        DistributionStatePreflightService::verify(&accounts_file, plan.current_cli_id.as_deref())?;
        DesktopSessionVerificationService::new(self.lifecycle.as_ref(), &home)
            .verify_plan_before_mutation(plan, request)?;
        DistributionAccountCommitService::validate_targets(
            &accounts_file,
            plan.target_app_id.as_deref(),
            plan.target_cli_id.as_deref(),
        )?;
        DistributionSharedAuthGuard::before_journal(self.lifecycle.as_ref(), plan)?;
        if plan.restart_required {
            DistributionDesktopAuthHandoffService::verify_before_stop(
                &accounts_file,
                plan.current_app_id
                    .as_deref()
                    .ok_or("Current Desktop account identity is unavailable")?,
                plan.current_cli_id
                    .as_deref()
                    .ok_or("Current CLI account identity is unavailable")?,
            )?;
        }

        let mut journal = DistributionJournal::create(
            &home,
            op_id,
            trigger_str,
            &request.reason,
            plan.target_app_id.as_deref(),
            plan.target_cli_id.as_deref(),
        )?;

        let mut restarted_desktop = false;
        let mut recovery_error: Option<String> = None;
        let mut commit_verified = true;
        if plan.restart_required {
            let outcome =
                DistributionDesktopSwitchService::new(self.lifecycle.as_ref(), &self.logger).run(
                    &home,
                    &mut journal,
                    plan,
                    request,
                    &mut accounts_file,
                    op_id,
                )?;
            restarted_desktop = outcome.restarted_desktop;
            recovery_error = outcome.recovery_error;
            commit_verified = outcome.commit_verified;
        } else {
            DistributionOfflineCommitService::new(self.lifecycle.as_ref(), &self.logger, op_id)
                .run(&home, &mut journal, plan, request, &mut accounts_file)?;
        }

        let _ = recovery::arm_automation_cooldown();
        if commit_verified {
            if let Err(error) = DistributionJournal::clear(&home) {
                recovery_error
                    .get_or_insert(format!("Distribution journal cleanup failed: {error}"));
            }
        }
        if accounts_file.settings.notify_on_switch && commit_verified && recovery_error.is_none() {
            self.lifecycle.notify_distribution_complete();
        }

        let status = if !commit_verified {
            DistributionStatus::Failed
        } else if recovery_error.is_some() {
            DistributionStatus::PartialSuccess
        } else {
            DistributionStatus::Success
        };

        let msg = if !commit_verified {
            "Desktop account change is unverified; credentials and Desktop state require inspection"
                .to_string()
        } else if recovery_error.is_some() {
            "Accounts distributed, but desktop recovery verification is incomplete".to_string()
        } else {
            "Accounts distributed successfully".to_string()
        };

        self.logger.log_action(
            op_id,
            "OUTCOME",
            trigger_str,
            &request.reason,
            &format!("status={:?} message={}", status, msg),
        );

        Ok(DistributionOutcome {
            operation_id: op_id.to_string(),
            status,
            trigger: trigger_str.to_string(),
            reason: request.reason.clone(),
            current_app_id: plan.current_app_id.clone(),
            current_cli_id: plan.current_cli_id.clone(),
            target_app_id: plan.target_app_id.clone(),
            target_cli_id: plan.target_cli_id.clone(),
            restarted_desktop,
            recovery_error,
            message: msg,
        })
    }
}
