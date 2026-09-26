use super::app_lifecycle::AppLifecycle;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_checkpoint_service::DistributionCheckpointService;
use super::distribution_desktop_auth_handoff_service::DistributionDesktopAuthHandoffService;
use super::distribution_desktop_relaunch_service::DistributionDesktopRelaunchService;
use super::distribution_desktop_switch_outcome::DistributionDesktopSwitchOutcome;
use super::distribution_journal::DistributionJournal;
use super::distribution_plan::DistributionPlan;
use super::distribution_recovery_audit_service::DistributionRecoveryAuditService;
use super::distribution_recovery_preflight_service::DistributionRecoveryPreflightService;
use super::distribution_request::DistributionRequest;
use super::distribution_shared_auth_guard::DistributionSharedAuthGuard;
use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountsFile, AuthJson};
use crate::{recovery, switcher};
use std::path::Path;

pub(super) struct DistributionDesktopSwitchService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
    recovery_preflight: DistributionRecoveryPreflightService<'a>,
}

impl<'a> DistributionDesktopSwitchService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
    ) -> Self {
        Self {
            lifecycle,
            logger,
            recovery_preflight: DistributionRecoveryPreflightService::new(lifecycle, logger),
        }
    }

    #[cfg(test)]
    pub(super) fn with_preflight(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
        preflight_dispatch: fn() -> Result<(), String>,
    ) -> Self {
        Self {
            lifecycle,
            logger,
            recovery_preflight: DistributionRecoveryPreflightService::with_dispatch(
                lifecycle,
                logger,
                preflight_dispatch,
            ),
        }
    }

    pub(super) fn run(
        &self,
        home: &Path,
        journal: &mut DistributionJournal,
        plan: &DistributionPlan,
        request: &DistributionRequest,
        accounts: &mut AccountsFile,
        operation_id: &str,
    ) -> Result<DistributionDesktopSwitchOutcome, String> {
        let trigger = request.trigger.as_str();
        let target_id = plan
            .target_app_id
            .as_deref()
            .ok_or("Target app ID missing")?;
        journal.update_phase(home, "stopping_desktop")?;
        self.logger.log_action(
            operation_id,
            "SHUTDOWN",
            trigger,
            &request.reason,
            "Stopping Desktop app gracefully",
        );
        let running_threads = switcher::detect_in_progress_threads();
        let capture_mode = match DistributionRecoveryAuditService::capture_window_bounds(
            self.logger,
            self.lifecycle,
            operation_id,
            trigger,
            &request.reason,
            &running_threads,
            accounts.settings.preserve_window_bounds_on_restart,
        ) {
            Ok(mode) => mode,
            Err(error) => {
                let _ = DistributionJournal::clear(home);
                return Err(format!(
                    "Could not capture ChatGPT window before shutdown: {error}"
                ));
            }
        };
        let checkpoint =
            match DistributionCheckpointService::prepare(home, self.lifecycle, &running_threads) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    self.lifecycle.abort_recovery();
                    return Err(error);
                }
            };
        self.recovery_preflight
            .run(home, &checkpoint, &running_threads, operation_id, request)?;
        let _ = recovery::arm_automation_cooldown();

        if let Err(error) = self.lifecycle.stop_app() {
            self.lifecycle.abort_recovery();
            self.logger.log_failure(
                operation_id,
                "SHUTDOWN_FAILED",
                trigger,
                &request.reason,
                "Desktop shutdown failed",
            );
            let message = format!("Could not stop ChatGPT Desktop gracefully: {error}");
            return Err(if error.before_signal {
                DistributionCheckpointService::rollback_and_clear(home, &checkpoint, message)
            } else {
                message
            });
        }
        if let Err(error) = DistributionCheckpointService::finalize_after_stop(&running_threads) {
            return Err(self.abort_before_auth_commit(home, accounts, plan, error, false));
        }
        let handoff = DistributionSharedAuthGuard::require_desktop_stopped(self.lifecycle)
            .and_then(|_| {
                let current_id = plan
                    .current_app_id
                    .as_deref()
                    .ok_or("Current Desktop account identity is unavailable")?;
                DistributionDesktopAuthHandoffService::preserve_after_stop(accounts, current_id)
            });
        if let Err(error) = handoff {
            return Err(self.abort_before_auth_commit(home, accounts, plan, error, true));
        }
        let target_account =
            match DistributionAccountCommitService::find_account(accounts, target_id) {
                Ok(account) => account,
                Err(error) => {
                    return Err(self.abort_before_auth_commit(home, accounts, plan, error, false))
                }
            };
        if let Err(error) = journal.update_phase(home, "auth_commit_app") {
            return Err(self.abort_before_auth_commit(home, accounts, plan, error, false));
        }
        self.logger.log_action(
            operation_id,
            "AUTH_COMMIT_APP",
            trigger,
            &request.reason,
            &format!(
                "Target app account_ref={}",
                LogRedactionService::sanitize_field("account_id", target_id)
            ),
        );
        let (previous_auth, committed_auth) =
            match DistributionAccountCommitService::apply_auth_tokens(
                self.lifecycle,
                &target_account,
            ) {
                Ok(snapshot) => snapshot,
                Err(error) => {
                    return Err(self.abort_before_auth_commit(home, accounts, plan, error, false))
                }
            };
        if let Err(error) = journal.update_phase(home, "relaunching_desktop") {
            return Err(self.abort_after_auth_commit(home, &previous_auth, &committed_auth, error));
        }
        self.logger.log_action(
            operation_id,
            "RELAUNCH",
            trigger,
            &request.reason,
            "Relaunching Desktop app",
        );
        let (restarted_desktop, recovery_error) =
            DistributionDesktopRelaunchService::new(self.lifecycle, self.logger).run(
                home,
                target_id,
                plan.target_cli_id.as_deref(),
                &running_threads,
                capture_mode,
                operation_id,
                request,
            );
        if !restarted_desktop {
            let reason = recovery_error.unwrap_or_else(|| "Desktop did not relaunch".into());
            return match DistributionAccountCommitService::rollback_and_relaunch_previous(
                self.lifecycle,
                &previous_auth,
                &committed_auth,
            ) {
                Ok(()) => {
                    self.lifecycle.abort_recovery();
                    DistributionJournal::clear(home)?;
                    Err(format!(
                        "Desktop switch rolled back after launch failure: {reason}"
                    ))
                }
                Err(rollback_error) => Ok(DistributionDesktopSwitchOutcome {
                    restarted_desktop: false,
                    recovery_error: Some(format!(
                        "Desktop launch failed: {reason}; rollback unverified: {rollback_error}"
                    )),
                    commit_verified: false,
                }),
            };
        }

        let registry_result = DistributionAccountCommitService::commit_latest_desktop_auth(
            self.lifecycle,
            home,
            accounts,
            target_id,
        );
        match registry_result {
            Ok(()) => Ok(DistributionDesktopSwitchOutcome {
                restarted_desktop: true,
                recovery_error,
                commit_verified: true,
            }),
            Err(error) => Ok(DistributionDesktopSwitchOutcome {
                restarted_desktop: true,
                recovery_error: Some(match recovery_error {
                    Some(recovery) => format!("{recovery}; registry commit failed: {error}"),
                    None => format!("Registry commit failed: {error}"),
                }),
                commit_verified: false,
            }),
        }
    }

    fn abort_before_auth_commit(
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

    fn abort_after_auth_commit(
        &self,
        home: &Path,
        previous: &AuthJson,
        committed: &AuthJson,
        error: String,
    ) -> String {
        self.lifecycle.abort_recovery();
        let rollback = DistributionAccountCommitService::rollback_and_relaunch_previous(
            self.lifecycle,
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
