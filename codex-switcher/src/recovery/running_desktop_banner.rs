use super::{
    automation_guard::operation_id_for_banner, recovery_banner::RecoveryBanner,
    recovery_mode::RecoveryMode,
};
use crate::distribution::{SystemWindowRestoreBackend, WindowProcessValidationService};
use crate::switcher;
use std::process::Child;

pub(super) fn verify_helper_running(child: &mut Child) -> Result<(), String> {
    match child.try_wait() {
        Ok(None) => Ok(()),
        Ok(Some(_)) => Err("Recovery banner helper exited before IPC dispatch".into()),
        Err(_) => Err("Could not verify recovery banner helper liveness".into()),
    }
}

impl RecoveryBanner {
    pub(crate) fn ensure_visible_after_owner(
        &mut self,
        ids: &[String],
        mode: RecoveryMode,
    ) -> Result<(), String> {
        if self.has_visible_panel() {
            self.verify_panel_alive()?;
            let pids = switcher::current_codex_app_pids();
            if pids.len() != 1 {
                return Err("Recovery banner requires exactly one Codex main process".into());
            }
            let mut backend = SystemWindowRestoreBackend::new()?;
            let process = WindowProcessValidationService::inspect(&mut backend, pids[0])?;
            if process != self.panel_process()? {
                return Err("Desktop process changed while recovery banner was visible".into());
            }
            return Ok(());
        }
        self.ensure_visible_after_owner_with(mode == RecoveryMode::CapturedRestart, || {
            Self::start_for_running_desktop(
                &operation_id_for_banner("late_thread_recovery"),
                ids,
                "thread_recovery",
            )
        })
    }

    pub(crate) fn ensure_visible_after_owner_with(
        &mut self,
        allow_relaunch: bool,
        start: impl FnOnce() -> Result<Self, String>,
    ) -> Result<(), String> {
        if self.has_visible_panel() {
            return Ok(());
        }
        let mut next = start()?;
        if !allow_relaunch && next.expected_process() != self.expected_process() {
            return Err("Desktop process changed before recovery banner appeared".into());
        }
        if !next.has_visible_panel() {
            return Err(
                "Desktop has no visible window after owner mount; recovery deferred".into(),
            );
        }
        next.verify_panel_alive()?;
        self.replay_pending_statuses_into(&mut next)?;
        *self = next;
        Ok(())
    }

    /// Recovery in an already running Desktop does not restore window bounds.
    /// A launchd daemon may lack Accessibility permission even with healthy IPC.
    pub(crate) fn start_for_running_desktop(
        operation_id: &str,
        ids: &[String],
        reason: &str,
    ) -> Result<Self, String> {
        let pids = switcher::current_codex_app_pids();
        if pids.len() != 1 {
            return Err("Recovery banner requires exactly one Codex main process".into());
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        Self::start_with_backend(operation_id, ids, reason, pids[0], &mut backend)
    }

    pub(crate) fn start_with_backend(
        operation_id: &str,
        ids: &[String],
        reason: &str,
        pid: u32,
        backend: &mut SystemWindowRestoreBackend,
    ) -> Result<Self, String> {
        Self::start_with_backend_and_panel(
            operation_id,
            ids,
            reason,
            pid,
            backend,
            Self::start_without_restore,
        )
    }

    pub(crate) fn start_with_backend_and_panel(
        operation_id: &str,
        ids: &[String],
        reason: &str,
        pid: u32,
        backend: &mut SystemWindowRestoreBackend,
        start_panel: impl FnOnce(
            &str,
            &[String],
            &str,
            crate::distribution::WindowCapture,
        ) -> Result<Self, String>,
    ) -> Result<Self, String> {
        let process = WindowProcessValidationService::inspect(backend, pid)?;
        if ids.is_empty() {
            return Ok(Self::without_window(process));
        }
        match backend.capture_banner_window(process.clone()) {
            Ok(placement) => {
                WindowProcessValidationService::confirm(backend, &process)?;
                start_panel(operation_id, ids, reason, placement)
            }
            Err(error) if error == "WINDOW_NOT_FOUND" => {
                WindowProcessValidationService::confirm(backend, &process)?;
                crate::logger::log(
                    "WARN",
                    "RECOVERY",
                    "RECOVERY_BANNER_UNAVAILABLE: WindowServer capture failed",
                );
                Ok(Self::without_window(process))
            }
            Err(error) => Err(format!("Banner capture failed: {error}")),
        }
    }
}

#[cfg(test)]
#[path = "running_desktop_banner.test.rs"]
mod tests;
