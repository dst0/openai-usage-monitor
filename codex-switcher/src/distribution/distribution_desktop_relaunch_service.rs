use super::{
    app_lifecycle::AppLifecycle, desktop_app_session::DesktopAppSession,
    distribution_audit_logger::DistributionAuditLogger,
    distribution_desktop_auth_handoff_service::DistributionDesktopAuthHandoffService,
    distribution_plan::DistributionPlan,
    distribution_recovery_audit_service::DistributionRecoveryAuditService,
    distribution_request::DistributionRequest, log_redaction_service::LogRedactionService,
    window_capture_mode::WindowCaptureMode,
};
use std::path::Path;

pub(super) struct DistributionDesktopRelaunchService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    logger: &'a DistributionAuditLogger,
}

impl<'a> DistributionDesktopRelaunchService<'a> {
    pub(super) fn new(
        lifecycle: &'a dyn AppLifecycle,
        logger: &'a DistributionAuditLogger,
    ) -> Self {
        Self { lifecycle, logger }
    }

    pub(super) fn run(
        &self,
        home: &Path,
        plan: &DistributionPlan,
        running_threads: &[String],
        capture_mode: WindowCaptureMode,
        operation_id: &str,
        request: &DistributionRequest,
    ) -> (bool, Option<String>) {
        let mut recovery_error = None;
        let new_pids = match self.lifecycle.launch_app() {
            Ok(pids) => pids,
            Err(error) => {
                self.lifecycle.abort_recovery();
                self.logger.log_warning(
                    operation_id,
                    "RELAUNCH_FAILED",
                    request.trigger.as_str(),
                    &request.reason,
                    "Desktop relaunch failed",
                );
                return (false, Some(error));
            }
        };
        if new_pids.len() != 1 {
            self.lifecycle.abort_recovery();
            return (
                true,
                Some(format!(
                    "Codex relaunch must produce exactly one main process, got {new_pids:?}"
                )),
            );
        }
        let pid = new_pids[0];
        let before = match self.lifecycle.inspect_process(pid) {
            Ok(identity) => identity,
            Err(error) => {
                self.lifecycle.abort_recovery();
                return (true, Some(error));
            }
        };
        let Some(cli_account_id) = plan.target_cli_id.as_deref() else {
            self.lifecycle.abort_recovery();
            return (true, Some("CLI account identity is unavailable".into()));
        };
        let Some(app_account_id) = plan.target_app_id.as_deref() else {
            self.lifecycle.abort_recovery();
            return (true, Some("Desktop account identity is unavailable".into()));
        };
        let path = home.join("desktop-app-session.json");
        let previous = match DesktopAppSession::load_checked(&path) {
            Ok(previous) => previous,
            Err(error) => {
                self.lifecycle.abort_recovery();
                return (true, Some(error));
            }
        };
        let bound = DesktopAppSession::bound(app_account_id, cli_account_id, before.clone());
        if let Err(error) = bound.save(&path) {
            self.lifecycle.abort_recovery();
            let restore =
                DesktopAppSession::restore_after_failed_save(&path, previous.as_ref(), &bound);
            return (
                true,
                Some(match restore {
                    Ok(()) => error,
                    Err(restore) => {
                        format!("{error}; Desktop marker rollback unverified: {restore}")
                    }
                }),
            );
        }
        match self.lifecycle.inspect_process(pid) {
            Ok(current) if current == before => {}
            _ => {
                self.lifecycle.abort_recovery();
                let restore = Self::restore_previous_marker(&path, previous.as_ref());
                return (
                    true,
                    Some(Self::identity_error_with_marker_restore(restore)),
                );
            }
        }
        self.logger.log_action(
            operation_id,
            "DESKTOP_SESSION",
            request.trigger.as_str(),
            &request.reason,
            &format!(
                "Desktop session account_ref={}",
                LogRedactionService::sanitize_field("account_id", app_account_id)
            ),
        );
        if let Err(error) = DistributionRecoveryAuditService::restore_and_recover(
            self.logger,
            self.lifecycle,
            super::recovery_audit_context::RecoveryAuditContext {
                pid,
                targets: running_threads,
                capture_mode,
                operation_id,
                request,
            },
            || {
                let verified = (|| {
                    DistributionDesktopAuthHandoffService::verify_after_launch(app_account_id)?;
                    if self.lifecycle.inspect_process(pid)? != before
                        || DesktopAppSession::load_checked(&path)?.as_ref() != Some(&bound)
                    {
                        return Err("Relaunched Desktop identity changed before recovery".into());
                    }
                    Ok(())
                })();
                if let Err(error) = verified {
                    // A planned target marker must not remain process-bound
                    // when Desktop has actually retained different auth.
                    let invalidation =
                        DesktopAppSession::restore_after_failed_save(&path, None, &bound);
                    return Err(match invalidation {
                        Ok(()) => error,
                        Err(invalidation) => {
                            format!(
                                "{error}; Desktop marker invalidation unverified: {invalidation}"
                            )
                        }
                    });
                }
                Ok(())
            },
        ) {
            recovery_error = Some(error);
        }
        match self.lifecycle.inspect_process(pid) {
            Ok(after) if after == before => {}
            _ => {
                self.lifecycle.abort_recovery();
                let restore = Self::restore_previous_marker(&path, previous.as_ref());
                let identity_error = Self::identity_error_with_marker_restore(restore);
                recovery_error = Some(match recovery_error {
                    Some(previous_error) => format!("{previous_error}; {identity_error}"),
                    None => identity_error,
                });
                return (true, recovery_error);
            }
        }
        (true, recovery_error)
    }

    fn restore_previous_marker(
        path: &Path,
        previous: Option<&DesktopAppSession>,
    ) -> Result<(), String> {
        match previous {
            Some(previous) => previous.save(path),
            None => match std::fs::remove_file(path) {
                Ok(()) => Ok(()),
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
                Err(error) => Err(error.to_string()),
            },
        }
    }

    fn identity_error_with_marker_restore(restore: Result<(), String>) -> String {
        match restore {
            Ok(()) => "Desktop process identity changed during recovery".into(),
            Err(error) => {
                format!("Desktop process identity changed; marker rollback failed: {error}")
            }
        }
    }
}
