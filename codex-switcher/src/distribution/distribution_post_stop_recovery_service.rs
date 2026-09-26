use super::app_lifecycle::AppLifecycle;
use super::desktop_session_verification_service::DesktopSessionVerificationService;
use super::distribution_account_commit_service::DistributionAccountCommitService;
use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_desktop_relaunch_service::DistributionDesktopRelaunchService;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::window_capture_mode::WindowCaptureMode;
use crate::models::{AccountsFile, AuthJson};
use crate::storage;
use std::path::Path;

/// Restarts the previous Desktop session if a required write fails after stop.
pub(super) struct DistributionPostStopRecoveryService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
    home: &'a Path,
    accounts: &'a AccountsFile,
    plan: &'a DistributionPlan,
    original_cli_auth: &'a AuthJson,
}

impl<'a> DistributionPostStopRecoveryService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
        home: &'a Path,
        accounts: &'a AccountsFile,
        plan: &'a DistributionPlan,
        original_cli_auth: &'a AuthJson,
    ) -> Self {
        Self {
            lifecycle,
            logger,
            home,
            accounts,
            plan,
            original_cli_auth,
        }
    }

    pub(super) fn recover(
        &self,
        original_error: String,
        running_threads: &[String],
        capture_mode: WindowCaptureMode,
        op_id: &str,
        request: &DistributionRequest,
    ) -> String {
        let mut errors = vec![original_error];
        let app_id = self
            .plan
            .current_app_id
            .as_deref()
            .or(self.plan.current_cli_id.as_deref());
        let Some(app_id) = app_id else {
            errors.push("Previous Desktop account identity is unavailable".into());
            self.emergency_launch(&mut errors);
            return errors.join("; ");
        };
        let app_account =
            match DistributionAccountCommitService::find_account(self.accounts, app_id) {
                Ok(account) => account,
                Err(error) => {
                    errors.push(error);
                    self.emergency_launch(&mut errors);
                    return errors.join("; ");
                }
            };
        if let Err(error) = DistributionAccountCommitService::apply_auth_tokens(&app_account) {
            errors.push(format!(
                "Previous Desktop authentication could not be staged: {error}"
            ));
            self.emergency_launch(&mut errors);
            return errors.join("; ");
        }

        let (launched, recovery_error) =
            DistributionDesktopRelaunchService::new(self.lifecycle, self.logger).run(
                self.home,
                app_id,
                running_threads,
                capture_mode,
                op_id,
                request,
            );
        if let Some(error) = recovery_error {
            errors.push(format!("Previous Desktop recovery is incomplete: {error}"));
        }
        if !launched {
            errors.push("Previous Desktop could not be relaunched".into());
        }
        if let Err(error) = storage::write_active_auth_json(self.original_cli_auth) {
            errors.push(format!(
                "Original CLI authentication could not be restored: {error}"
            ));
        } else if launched {
            if let Some(cli_id) = self.plan.current_cli_id.as_deref() {
                if let Err(error) =
                    DesktopSessionVerificationService::new(self.lifecycle, self.home)
                        .reconcile_cli_binding(cli_id)
                {
                    errors.push(format!(
                        "Original CLI binding could not be restored: {error}"
                    ));
                }
            }
        }
        errors.join("; ")
    }

    pub(super) fn relaunch_without_dispatch(&self, original_error: String) -> String {
        let mut errors = vec![format!(
            "Post-shutdown recovery checkpoint failed: {original_error}"
        )];
        let app_id = self
            .plan
            .current_app_id
            .as_deref()
            .or(self.plan.current_cli_id.as_deref());
        let Some(app_id) = app_id else {
            errors.push("Previous Desktop account identity is unavailable".into());
            self.emergency_launch(&mut errors);
            return errors.join("; ");
        };
        let app_account =
            match DistributionAccountCommitService::find_account(self.accounts, app_id) {
                Ok(account) => account,
                Err(error) => {
                    errors.push(error);
                    self.emergency_launch(&mut errors);
                    return errors.join("; ");
                }
            };
        if let Err(error) = DistributionAccountCommitService::apply_auth_tokens(&app_account) {
            errors.push(format!(
                "Previous Desktop authentication could not be staged: {error}"
            ));
            self.emergency_launch(&mut errors);
            return errors.join("; ");
        }

        let mut binding_written = false;
        match self.lifecycle.launch_app() {
            Ok(pids) if pids.len() == 1 => {
                let bind = DesktopSessionVerificationService::new(self.lifecycle, self.home)
                    .bind_relaunched_process(app_id, app_id, pids[0]);
                if let Err(error) = bind {
                    errors.push(format!("Previous Desktop account binding failed: {error}"));
                } else {
                    binding_written = true;
                    if let Err(error) = self.lifecycle.verify_desktop_stable(&pids, false) {
                        errors.push(format!("Previous Desktop stability failed: {error}"));
                    } else {
                        errors.push("Previous Desktop account relaunched and bound".into());
                    }
                }
            }
            Ok(pids) => errors.push(format!(
                "Previous Desktop relaunch produced {} main processes",
                pids.len()
            )),
            Err(error) => errors.push(format!("Previous Desktop relaunch failed: {error}")),
        }
        match storage::write_active_auth_json(self.original_cli_auth) {
            Ok(()) if binding_written => {
                if let Some(cli_id) = self.plan.current_cli_id.as_deref() {
                    if let Err(error) =
                        DesktopSessionVerificationService::new(self.lifecycle, self.home)
                            .reconcile_cli_binding(cli_id)
                    {
                        errors.push(format!(
                            "Original CLI binding could not be restored: {error}"
                        ));
                    }
                }
            }
            Ok(()) => {}
            Err(error) => errors.push(format!(
                "Original CLI authentication could not be restored: {error}"
            )),
        }
        errors.join("; ")
    }

    fn emergency_launch(&self, errors: &mut Vec<String>) {
        if let Err(error) = self.lifecycle.launch_app() {
            errors.push(format!("Emergency Desktop relaunch failed: {error}"));
        }
        self.lifecycle.abort_recovery();
    }
}
