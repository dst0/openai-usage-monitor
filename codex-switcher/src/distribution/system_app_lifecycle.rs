use super::app_lifecycle::AppLifecycle;
use super::app_stop_error::AppStopError;
use super::distribution_window_task_service::DistributionWindowTaskService;
use super::window_capture_failure_policy::{
    classify_capture_failure, optional_banner_capture_failure,
};
use super::window_capture_mode::WindowCaptureMode;
use super::window_task_restore_phase::WindowTaskRestorePhase;
use crate::distribution::{
    RestoreOutcome, SystemWindowRestoreBackend, WindowProcessValidationService,
    WindowRestoreService,
};
use crate::recovery::{self, RecoveryBanner, RecoveryMode};
use crate::switcher;
use std::path::PathBuf;
use std::sync::Mutex;

pub struct SystemAppLifecycle {
    recovery_banner: Mutex<Option<RecoveryBanner>>,
    window_tasks: DistributionWindowTaskService,
}

impl Default for SystemAppLifecycle {
    fn default() -> Self {
        Self {
            recovery_banner: Mutex::new(None),
            window_tasks: DistributionWindowTaskService::default(),
        }
    }
}

impl SystemAppLifecycle {
    fn expected_process(&self) -> Result<super::WindowProcessIdentity, String> {
        Ok(self
            .recovery_banner
            .lock()
            .map_err(|_| "Recovery banner state lock is poisoned".to_string())?
            .as_ref()
            .ok_or("Desktop operation has no captured process identity")?
            .expected_process()
            .clone())
    }

    fn capture_window_tasks_with(
        &self,
        backend: &mut SystemWindowRestoreBackend,
        desktop_home: Result<PathBuf, String>,
    ) -> Result<(), String> {
        let expected = self.expected_process()?;
        self.window_tasks
            .capture_with(&expected, backend, desktop_home)
    }
}

impl AppLifecycle for SystemAppLifecycle {
    fn is_app_running(&self) -> Result<bool, String> {
        switcher::is_shared_auth_active_checked()
    }

    fn preflight_shutdown_windows(&self) -> Result<(), String> {
        let expected = self.expected_process()?;
        let captured = self.window_tasks.captured_window_ids()?;
        switcher::preflight_shutdown_windows(&expected, captured.as_deref())
    }

    fn stop_app(&self) -> Result<(), AppStopError> {
        let expected = self.expected_process().map_err(AppStopError::before)?;
        if switcher::current_codex_app_pids() != [expected.pid] {
            return Err(AppStopError::before(
                "Desktop process set changed before shutdown",
            ));
        }
        let mut backend = SystemWindowRestoreBackend::new().map_err(AppStopError::before)?;
        WindowProcessValidationService::confirm(&mut backend, &expected)
            .map_err(AppStopError::before)?;
        let captured = self
            .window_tasks
            .captured_window_ids()
            .map_err(AppStopError::before)?;
        switcher::stop_codex_app_gracefully_with(&expected, captured.as_deref(), || {
            self.window_tasks.verify_current(&expected)
        })
    }

    fn launch_app(&self) -> Result<Vec<u32>, String> {
        switcher::launch_codex_app()
    }

    fn inspect_process(
        &self,
        pid: u32,
    ) -> Result<super::window_restore_process_identity::ProcessIdentity, String> {
        if switcher::current_codex_app_pids() != [pid] {
            return Err("Desktop process set changed during account binding".into());
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        WindowProcessValidationService::inspect(&mut backend, pid)
    }

    fn capture_window_bounds(
        &self,
        operation_id: &str,
        targets: &[String],
        reason: &str,
        preserve_window_bounds: bool,
    ) -> Result<WindowCaptureMode, String> {
        let pids = switcher::current_codex_app_pids();
        if pids.len() != 1 {
            return Err(format!(
                "Window capture requires exactly one Codex process, got {pids:?}"
            ));
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        if !preserve_window_bounds {
            let process = WindowProcessValidationService::inspect(&mut backend, pids[0])?;
            let banner = match backend.capture_banner_window(process.clone()) {
                Ok(placement) => {
                    if placement.process != process {
                        return Err("Banner window process identity changed".into());
                    }
                    WindowProcessValidationService::confirm(&mut backend, &process)?;
                    RecoveryBanner::start_without_restore(operation_id, targets, reason, placement)
                        .unwrap_or_else(|_| {
                            crate::logger::log(
                                "WARN",
                                "RECOVERY",
                                "RECOVERY_BANNER_UNAVAILABLE: native panel did not become visible",
                            );
                            RecoveryBanner::without_window(process)
                        })
                }
                Err(error) if optional_banner_capture_failure(&error) => {
                    if error != "WINDOW_NOT_FOUND" {
                        crate::logger::log(
                            "WARN",
                            "RECOVERY",
                            "RECOVERY_BANNER_UNAVAILABLE: WindowServer capture failed",
                        );
                    }
                    RecoveryBanner::without_window(process)
                }
                Err(error) => return Err(format!("Banner capture failed: {error}")),
            };
            let mut current = self
                .recovery_banner
                .lock()
                .map_err(|_| "Recovery banner state lock is poisoned".to_string())?;
            if current.is_some() {
                return Err("A previous recovery banner is still active".into());
            }
            *current = Some(banner);
            return Ok(WindowCaptureMode::Skipped);
        }
        let result = WindowRestoreService::new(Default::default())?.capture(
            &mut backend,
            operation_id,
            reason,
            pids[0],
        );
        if result.report.outcome != RestoreOutcome::Restored
            && classify_capture_failure(&result.report)? == WindowCaptureMode::Absent
        {
            let mut current = self
                .recovery_banner
                .lock()
                .map_err(|_| "Recovery banner state lock is poisoned".to_string())?;
            if current.is_some() {
                return Err("A previous recovery banner is still active".into());
            }
            let process = WindowProcessValidationService::inspect(&mut backend, pids[0])?;
            *current = Some(RecoveryBanner::without_window(process));
            return Ok(WindowCaptureMode::Absent);
        }
        let capture = result
            .capture
            .ok_or_else(|| "Codex window capture returned no frame".to_string())?;
        let banner = RecoveryBanner::start_with_capture(operation_id, targets, reason, capture)?;
        let mut current = self
            .recovery_banner
            .lock()
            .map_err(|_| "Recovery banner state lock is poisoned".to_string())?;
        if current.is_some() {
            return Err("A previous recovery banner is still active".into());
        }
        *current = Some(banner);
        Ok(WindowCaptureMode::Captured)
    }

    fn restore_window_bounds(
        &self,
        pid: u32,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let current = self
            .recovery_banner
            .lock()
            .map_err(|_| "Recovery banner state lock is poisoned".to_string())?;
        let banner = current
            .as_ref()
            .ok_or_else(|| "Window restore has no pre-shutdown capture".to_string())?;
        banner.restore_after_relaunch(pid, operation_id, reason)
    }

    fn capture_window_tasks(&self) -> Result<(), String> {
        let mut backend = SystemWindowRestoreBackend::new()?;
        let home = super::window_task_probe_service::WindowTaskProbeService::desktop_codex_home(
            crate::storage::codex_home(),
            super::window_task_probe_service::WindowTaskProbeService::account_home(),
        );
        self.capture_window_tasks_with(&mut backend, home)
    }

    fn captured_window_task_count(&self) -> Result<usize, String> {
        self.window_tasks
            .captured_window_ids()
            .map(|ids| ids.unwrap_or_default().len())
    }

    fn restore_window_tasks(
        &self,
        bound: &super::desktop_app_session::DesktopAppSession,
        phase: WindowTaskRestorePhase<'_>,
    ) {
        let verified = bound
            .process
            .as_ref()
            .ok_or("Desktop session has no bound process".into())
            .and_then(|process| {
                self.inspect_process(process.pid).and_then(|observed| {
                    if observed == *process {
                        Ok(observed)
                    } else {
                        Err("Relaunched Desktop process changed before window task restore".into())
                    }
                })
            });
        self.window_tasks.restore(verified, bound, phase);
    }

    fn finish_window_tasks(&self) -> Result<(), String> {
        self.window_tasks.finish()
    }

    fn rebind_banner(&self, pid: u32) -> Result<(), String> {
        let current = self
            .recovery_banner
            .lock()
            .map_err(|_| "Recovery banner state lock is poisoned".to_string())?;
        current
            .as_ref()
            .ok_or_else(|| "Recovery has no active banner".to_string())?
            .update_process(pid)
    }

    fn abort_recovery(&self) {
        let banner = self.recovery_banner.lock().ok().and_then(|mut b| b.take());
        drop(banner);
    }

    fn recover_threads(&self, targets: &[String]) -> Result<(), String> {
        let mut banner = self
            .recovery_banner
            .lock()
            .map_err(|_| "Recovery banner state lock is poisoned".to_string())?
            .take()
            .ok_or_else(|| "Recovery has no active banner".to_string())?;
        let rec_res = recovery::recover_threads_with_banner(
            targets,
            RecoveryMode::CapturedRestart,
            &mut banner,
        );
        drop(banner);
        rec_res
    }

    fn verify_desktop_stable(&self, pids: &[u32], require_window: bool) -> Result<(), String> {
        recovery::verify_desktop_stable(pids, require_window)
    }

    fn notify_distribution_complete(&self) {
        switcher::send_macos_notification(
            "Codex Account Distribution",
            "Automatic account distribution completed",
        );
    }
}

#[cfg(test)]
#[path = "system_app_lifecycle.test.rs"]
mod tests;
