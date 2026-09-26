use super::{
    app_lifecycle::AppLifecycle, desktop_app_session::DesktopAppSession,
    distribution_audit_logger::DistributionAuditLogger,
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
        app_account_id: &str,
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
        let process = match self.lifecycle.inspect_process(pid) {
            Ok(after) if after == before => after,
            _ => {
                self.lifecycle.abort_recovery();
                return (
                    true,
                    Some("Desktop process identity changed before session binding".into()),
                );
            }
        };
        let path = home.join("desktop-app-session.json");
        if let Err(error) =
            DesktopAppSession::bound(app_account_id, app_account_id, process.clone()).save(&path)
        {
            self.lifecycle.abort_recovery();
            return (true, Some(error));
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
            pid,
            running_threads,
            capture_mode,
            operation_id,
            request,
        ) {
            recovery_error.get_or_insert(error);
        }
        if !matches!(self.lifecycle.inspect_process(pid), Ok(after) if after == process) {
            recovery_error
                .get_or_insert_with(|| "Desktop process identity changed during recovery".into());
        }
        (true, recovery_error)
    }
}
