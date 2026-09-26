use super::app_lifecycle::AppLifecycle;
use super::desktop_session_verification_service::DesktopSessionVerificationService;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_cli_commit_service::DistributionCliCommitService;
use super::distribution_desktop_relaunch_service::DistributionDesktopRelaunchService;
use super::distribution_journal::DistributionJournal;
use super::distribution_outcome::{DistributionOutcome, DistributionStatus};
use super::distribution_plan::DistributionPlan;
use super::distribution_post_stop_recovery_service::DistributionPostStopRecoveryService;
use super::distribution_recovery_audit_service::DistributionRecoveryAuditService;
use super::distribution_request::DistributionRequest;
use super::distribution_state_preflight_service::DistributionStatePreflightService;
use super::log_redaction_service::LogRedactionService;
use super::system_app_lifecycle::SystemAppLifecycle;
use crate::models::AccountsFile;
use crate::recovery;
use crate::storage;
use crate::switcher;
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
        mut accounts_file: AccountsFile,
    ) -> Result<DistributionOutcome, String> {
        let home = storage::codex_home();
        let trigger_str = request.trigger.as_str();

        let _lock = recovery::operation_lock()?;
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
        let original_cli_auth = if plan.restart_required {
            Some(storage::read_active_auth_json()?)
        } else {
            None
        };

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
        if plan.restart_required {
            let target_app_id = plan.target_app_id.as_ref().ok_or("Target app ID missing")?;
            let target_acc =
                DistributionAccountCommitService::find_account(&accounts_file, target_app_id)?;
            journal.update_phase(&home, "stopping_desktop")?;
            self.logger.log_action(
                op_id,
                "SHUTDOWN",
                trigger_str,
                &request.reason,
                "Stopping Desktop app gracefully",
            );
            let running_threads = switcher::detect_in_progress_threads();
            let capture_mode = match DistributionRecoveryAuditService::capture_window_bounds(
                &self.logger,
                self.lifecycle.as_ref(),
                op_id,
                trigger_str,
                &request.reason,
                &running_threads,
                accounts_file.settings.preserve_window_bounds_on_restart,
            ) {
                Ok(mode) => mode,
                Err(error) => {
                    let _ = DistributionJournal::clear(&home);
                    return Err(format!(
                        "Could not capture Codex window before shutdown: {error}"
                    ));
                }
            };
            if let Err(error) = recovery::save_pending(&running_threads) {
                self.lifecycle.abort_recovery();
                let _ = DistributionJournal::clear(&home);
                return Err(error);
            }
            let _ = recovery::arm_automation_cooldown();

            if !running_threads.is_empty() {
                let _ = recovery::preflight_desktop_dispatch();
            }

            if let Err(e) = self.lifecycle.stop_app() {
                self.lifecycle.abort_recovery();
                let _ = recovery::save_pending(&[]);
                let _ = DistributionJournal::clear(&home);
                self.logger.log_failure(
                    op_id,
                    "SHUTDOWN_FAILED",
                    trigger_str,
                    &request.reason,
                    "Desktop shutdown failed",
                );
                return Err(format!("Could not stop Codex Desktop gracefully: {e}"));
            }
            if let Err(error) = recovery::save_pending(&running_threads) {
                self.lifecycle.abort_recovery();
                let mut recovered_error = DistributionPostStopRecoveryService::new(
                    self.lifecycle.as_ref(),
                    &self.logger,
                    &home,
                    &accounts_file,
                    plan,
                    original_cli_auth
                        .as_ref()
                        .ok_or("Original CLI auth is missing")?,
                )
                .relaunch_without_dispatch(error);
                if let Err(clear_error) = DistributionJournal::clear(&home) {
                    recovered_error.push_str(&format!("; journal cleanup failed: {clear_error}"));
                }
                return Err(recovered_error);
            }
            let post_stop_write = (|| -> Result<(), String> {
                journal.update_phase(&home, "auth_commit_app")?;
                self.logger.log_action(
                    op_id,
                    "AUTH_COMMIT_APP",
                    trigger_str,
                    &request.reason,
                    &format!(
                        "Target app account_ref={}",
                        LogRedactionService::sanitize_field("account_id", target_app_id)
                    ),
                );
                DistributionAccountCommitService::apply_auth_tokens(&target_acc)?;
                journal.update_phase(&home, "relaunching_desktop")
            })();
            if let Err(error) = post_stop_write {
                let mut recovered_error = DistributionPostStopRecoveryService::new(
                    self.lifecycle.as_ref(),
                    &self.logger,
                    &home,
                    &accounts_file,
                    plan,
                    original_cli_auth
                        .as_ref()
                        .ok_or("Original CLI auth is missing")?,
                )
                .recover(error, &running_threads, capture_mode, op_id, request);
                if let Err(clear_error) = DistributionJournal::clear(&home) {
                    recovered_error.push_str(&format!("; journal cleanup failed: {clear_error}"));
                }
                return Err(recovered_error);
            }
            self.logger.log_action(
                op_id,
                "RELAUNCH",
                trigger_str,
                &request.reason,
                "Relaunching Desktop app",
            );
            (restarted_desktop, recovery_error) =
                DistributionDesktopRelaunchService::new(self.lifecycle.as_ref(), &self.logger).run(
                    &home,
                    target_app_id,
                    &running_threads,
                    capture_mode,
                    op_id,
                    request,
                );

            if let Some(error) =
                DistributionCliCommitService::new(self.lifecycle.as_ref(), &self.logger, &home)
                    .after_relaunch(
                        &mut accounts_file,
                        plan,
                        original_cli_auth
                            .as_ref()
                            .ok_or("Original CLI auth is missing")?,
                        &mut journal,
                        op_id,
                        request,
                    )?
            {
                recovery_error.get_or_insert(error);
            }
        } else {
            let cli_result =
                DistributionCliCommitService::new(self.lifecycle.as_ref(), &self.logger, &home)
                    .without_restart(&mut accounts_file, plan, &mut journal, op_id, request);
            let cli_result = match cli_result {
                Ok(result) => result,
                Err(mut error) => {
                    if let Err(clear_error) = DistributionJournal::clear(&home) {
                        error.push_str(&format!("; journal cleanup failed: {clear_error}"));
                    }
                    return Err(error);
                }
            };
            if let Some(error) = cli_result {
                recovery_error.get_or_insert(error);
            }
        }

        let _ = recovery::arm_automation_cooldown();
        let _ = DistributionJournal::clear(&home);
        if accounts_file.settings.notify_on_switch {
            self.lifecycle.notify_distribution_complete();
        }

        let status = if recovery_error.is_some() {
            DistributionStatus::PartialSuccess
        } else {
            DistributionStatus::Success
        };

        let msg = if recovery_error.is_some() {
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
            target_app_id: if plan.current_app_id.is_none() && !plan.restart_required {
                None
            } else {
                plan.target_app_id.clone()
            },
            target_cli_id: accounts_file.active_account_id.clone(),
            restarted_desktop,
            recovery_error,
            message: msg,
        })
    }
}
