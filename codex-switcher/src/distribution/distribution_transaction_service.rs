use super::app_lifecycle::AppLifecycle;
use super::desktop_app_session::DesktopAppSession;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_journal::DistributionJournal;
use super::distribution_outcome::{DistributionOutcome, DistributionStatus};
use super::distribution_plan::DistributionPlan;
use super::distribution_recovery_audit_service::DistributionRecoveryAuditService;
use super::distribution_request::DistributionRequest;
use super::log_redaction_service::LogRedactionService;
use super::system_app_lifecycle::SystemAppLifecycle;
use crate::models::AccountsFile;
use crate::recovery;
use crate::storage::{self, save_accounts};
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
            let _ = recovery::save_pending(&running_threads);
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

            journal.update_phase(&home, "relaunching_desktop")?;
            self.logger.log_action(
                op_id,
                "RELAUNCH",
                trigger_str,
                &request.reason,
                "Relaunching Desktop app",
            );
            match self.lifecycle.launch_app() {
                Ok(new_pids) => {
                    restarted_desktop = true;
                    if new_pids.len() != 1 {
                        recovery_error = Some(format!(
                            "Codex relaunch must produce exactly one main process, got {new_pids:?}"
                        ));
                        self.lifecycle.abort_recovery();
                    } else {
                        if let Err(error) = DistributionRecoveryAuditService::restore_and_recover(
                            &self.logger,
                            self.lifecycle.as_ref(),
                            new_pids[0],
                            &running_threads,
                            capture_mode,
                            op_id,
                            request,
                        ) {
                            recovery_error = Some(error);
                        }
                    }
                }
                Err(e) => {
                    recovery_error = Some(e.clone());
                    self.lifecycle.abort_recovery();
                    self.logger.log_warning(
                        op_id,
                        "RELAUNCH_FAILED",
                        trigger_str,
                        &request.reason,
                        "Desktop relaunch failed",
                    );
                }
            }

            let session_path = home.join("desktop-app-session.json");
            let _ = DesktopAppSession::new(target_app_id).save(&session_path);
            self.logger.log_action(
                op_id,
                "DESKTOP_SESSION",
                trigger_str,
                &request.reason,
                &format!(
                    "Desktop session account_ref={}",
                    LogRedactionService::sanitize_field("account_id", target_app_id)
                ),
            );

            if let Some(target_cli_id) = &plan.target_cli_id {
                if !target_cli_id.eq_ignore_ascii_case(target_app_id) {
                    journal.update_phase(&home, "auth_commit_cli")?;
                    let cli_acc = DistributionAccountCommitService::find_account(
                        &accounts_file,
                        target_cli_id,
                    )?;
                    DistributionAccountCommitService::apply_auth_tokens(&cli_acc)?;
                    accounts_file.active_account_id = Some(cli_acc.id.clone());
                    let _ = save_accounts(&accounts_file);
                    self.logger.log_action(
                        op_id,
                        "AUTH_COMMIT_CLI",
                        trigger_str,
                        &request.reason,
                        &format!(
                            "Target CLI account_ref={} no_second_restart=true",
                            LogRedactionService::sanitize_field("account_id", target_cli_id)
                        ),
                    );
                } else {
                    accounts_file.active_account_id = Some(target_acc.id.clone());
                    let _ = save_accounts(&accounts_file);
                }
            }
        } else {
            if plan.cli_switch_needed {
                if let Some(target_cli_id) = &plan.target_cli_id {
                    journal.update_phase(&home, "auth_commit_cli")?;
                    let cli_acc = DistributionAccountCommitService::find_account(
                        &accounts_file,
                        target_cli_id,
                    )?;
                    DistributionAccountCommitService::apply_auth_tokens(&cli_acc)?;
                    accounts_file.active_account_id = Some(cli_acc.id.clone());
                    let _ = save_accounts(&accounts_file);
                    self.logger.log_action(
                        op_id,
                        "AUTH_COMMIT_CLI",
                        trigger_str,
                        &request.reason,
                        &format!(
                            "CLI switched account_ref={}",
                            LogRedactionService::sanitize_field("account_id", target_cli_id)
                        ),
                    );
                }
            }
            if plan.app_switch_needed {
                if let Some(target_app_id) = &plan.target_app_id {
                    let session_path = home.join("desktop-app-session.json");
                    let _ = DesktopAppSession::new(target_app_id).save(&session_path);
                    self.logger.log_action(
                        op_id,
                        "DESKTOP_SESSION",
                        trigger_str,
                        &request.reason,
                        &format!(
                            "Desktop session account_ref={}",
                            LogRedactionService::sanitize_field("account_id", target_app_id)
                        ),
                    );
                }
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
            target_app_id: plan.target_app_id.clone(),
            target_cli_id: plan.target_cli_id.clone(),
            restarted_desktop,
            recovery_error,
            message: msg,
        })
    }
}
