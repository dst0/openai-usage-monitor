use crate::distribution::{
    RestoreOutcome, SystemWindowRestoreBackend, WindowCapture, WindowProcessIdentity,
    WindowRelaunchRestoreService, WindowRestoreBackend, WindowRestoreService,
};
use crate::recovery_banner::{
    BannerSessionStatus, ProcessIdentity as BannerProcessIdentity, RecoveryBannerService,
    RecoverySession, RecoverySessionCatalog, SavedWindow, WindowRect,
};
use crate::{storage, switcher};
use std::{
    path::PathBuf,
    process::{Child, Command, Stdio},
    thread::sleep,
    time::{Duration, Instant},
};
const MIN_BANNER_VISIBLE: Duration = Duration::from_secs(5);

pub(crate) struct RecoveryBanner {
    expected_process: WindowProcessIdentity,
    child: Option<Child>,
    visible_since: Option<Instant>,
    service: Option<RecoveryBannerService>,
    capture: Option<WindowCapture>,
    pub(super) pending_statuses: Vec<(String, BannerSessionStatus)>,
}

impl RecoveryBanner {
    pub(crate) fn without_window(expected_process: WindowProcessIdentity) -> Self {
        Self {
            expected_process,
            child: None,
            visible_since: None,
            service: None,
            capture: None,
            pending_statuses: Vec::new(),
        }
    }

    pub(crate) fn expected_process(&self) -> &WindowProcessIdentity {
        &self.expected_process
    }

    pub(crate) fn has_visible_panel(&self) -> bool {
        self.service.is_some() && self.visible_since.is_some()
    }

    pub(crate) fn verify_panel_alive(&mut self) -> Result<(), String> {
        let child = self
            .child
            .as_mut()
            .ok_or("Recovery banner helper is absent")?;
        super::running_desktop_banner::verify_helper_running(child)
    }

    pub(crate) fn panel_process(&self) -> Result<WindowProcessIdentity, String> {
        let service = self
            .service
            .as_ref()
            .ok_or("Recovery banner has no visible panel")?;
        let process = service.read_payload()?.expected_process;
        WindowProcessIdentity::new(process.pid, process.birth_identity)
    }

    pub(crate) fn start(operation_id: &str, ids: &[String], reason: &str) -> Result<Self, String> {
        let pids = switcher::current_codex_app_pids();
        if pids.len() != 1 {
            return Err("Recovery banner requires exactly one Codex main process".into());
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        let capture_result = WindowRestoreService::new(Default::default())?.capture(
            &mut backend,
            operation_id,
            reason,
            pids[0],
        );
        if capture_result.report.outcome != RestoreOutcome::Restored {
            return Err("Codex window capture did not complete successfully".into());
        }
        let capture = capture_result
            .capture
            .ok_or_else(|| "Could not capture the exact Codex window for recovery".to_string())?;
        Self::start_with_capture(operation_id, ids, reason, capture)
    }

    pub(crate) fn start_with_capture(
        operation_id: &str,
        ids: &[String],
        reason: &str,
        capture: WindowCapture,
    ) -> Result<Self, String> {
        Self::start_with_placement(operation_id, ids, reason, capture, true)
    }

    pub(crate) fn start_without_restore(
        operation_id: &str,
        ids: &[String],
        reason: &str,
        placement: WindowCapture,
    ) -> Result<Self, String> {
        Self::start_with_placement(operation_id, ids, reason, placement, false)
    }

    fn start_with_placement(
        operation_id: &str,
        ids: &[String],
        _reason: &str,
        capture: WindowCapture,
        restore_bounds: bool,
    ) -> Result<Self, String> {
        if ids.is_empty() {
            return Ok(Self {
                expected_process: capture.process.clone(),
                child: None,
                visible_since: None,
                service: None,
                capture: restore_bounds.then_some(capture),
                pending_statuses: Vec::new(),
            });
        }
        let expected =
            BannerProcessIdentity::new(capture.process.pid, capture.process.birth_id.clone())?;
        let saved_window = SavedWindow::new(
            WindowRect::new(
                capture.frame.x,
                capture.frame.y,
                capture.frame.width,
                capture.frame.height,
            )?,
            WindowRect::new(
                capture.screen.frame.x,
                capture.screen.frame.y,
                capture.screen.frame.width,
                capture.screen.frame.height,
            )?,
        )?;
        let home = storage::codex_home();
        let service = RecoveryBannerService::begin(
            &home,
            operation_id,
            expected,
            saved_window,
            RecoverySessionCatalog::load(&home, ids),
        )?;
        if !restore_bounds {
            service.skip_window_restore()?;
        }
        let payload = service.payload_path().to_path_buf();
        let ready = payload
            .parent()
            .ok_or("Recovery banner payload has no parent")?
            .join("restore-banner.ready");
        let mut child = None;
        if let Some(path) = banner_helper_candidates().into_iter().next() {
            let _ = std::fs::remove_file(&ready);
            if let Ok(process) = Command::new(path)
                .args(["--payload", &payload.to_string_lossy()])
                .stdin(Stdio::null())
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
            {
                child = Some(process);
            }
        }
        let mut child =
            child.ok_or_else(|| "Recovery banner helper is not installed".to_string())?;
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if matches!(std::fs::read(&ready), Ok(ref bytes) if bytes == b"visible\n") {
                let _ = std::fs::remove_file(&ready);
                crate::runtime_print!(
                    "RECOVERY_BANNER_CONFIRMED operation_id={operation_id} targets={}",
                    ids.len()
                );
                return Ok(Self {
                    expected_process: capture.process.clone(),
                    child: Some(child),
                    visible_since: Some(Instant::now()),
                    service: Some(service),
                    capture: restore_bounds.then_some(capture),
                    pending_statuses: Vec::new(),
                });
            }
            if child.try_wait().ok().flatten().is_some() {
                break;
            }
            sleep(Duration::from_millis(50));
        }
        let _ = child.kill();
        let _ = child.wait();
        Err("Recovery banner helper did not confirm a visible panel".into())
    }

    pub(crate) fn update_process(&self, pid: u32) -> Result<(), String> {
        let Some(service) = &self.service else {
            return Ok(());
        };
        let mut backend = SystemWindowRestoreBackend::new()?;
        let process = backend.inspect_process(pid)?;
        let current = service.read_payload()?;
        service.update_target(
            BannerProcessIdentity::new(process.pid, process.birth_id.clone())?,
            current.saved_window,
        )
    }

    /// Restore the saved geometry against the exact process created by the
    /// relaunch, then rebind the single banner to that process identity.
    pub(crate) fn restore_after_relaunch(
        &self,
        pid: u32,
        operation_id: &str,
        reason: &str,
    ) -> Result<(), String> {
        let Some(capture) = self.capture.clone() else {
            return Ok(());
        };
        // Rebind the visible payload before any geometry writes so the banner
        // describes the process that now owns the window even on a partial
        // restore.
        self.update_process(pid)?;
        let mut backend = SystemWindowRestoreBackend::new()?;
        let service = WindowRestoreService::new(Default::default())?;
        let report = WindowRelaunchRestoreService::restore(
            &service,
            &mut backend,
            operation_id,
            reason,
            capture,
            pid,
        );
        if report.outcome != RestoreOutcome::Restored {
            let detail = report
                .events
                .last()
                .map(|event| event.detail.as_str())
                .unwrap_or("Window restore did not complete");
            return Err(format!("Window restore {:?}: {detail}", report.outcome));
        }
        Ok(())
    }

    pub(crate) fn update_status(
        &self,
        id: &str,
        status: BannerSessionStatus,
    ) -> Result<(), String> {
        let Some(service) = &self.service else {
            return Ok(());
        };
        let short_id = RecoverySession::from_raw("", "", id, status).short_id;
        service.update_status(&short_id, status)
    }
}

impl Drop for RecoveryBanner {
    fn drop(&mut self) {
        if let Some(visible_since) = self.visible_since {
            let elapsed = visible_since.elapsed();
            if elapsed < MIN_BANNER_VISIBLE {
                sleep(MIN_BANNER_VISIBLE - elapsed);
            }
        }
        if let Some(child) = &mut self.child {
            let _ = child.kill();
            let _ = child.wait();
        }
        if let Some(service) = self.service.take() {
            let _ = service.finish();
        }
    }
}

pub(super) fn banner_helper_candidates() -> Vec<PathBuf> {
    #[cfg(test)]
    crate::test_live_system::forbid("installed recovery-banner helper");
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::current_exe() {
        if let Some(parent) = path.parent() {
            candidates.push(parent.join("codex-recovery-banner"));
        }
    }
    if let Some(home) = dirs::home_dir() {
        candidates.push(home.join(".local/bin/codex-recovery-banner"));
    }
    candidates
        .into_iter()
        .filter(|path| path.is_file())
        .collect()
}

#[cfg(test)]
#[path = "recovery_banner.test.rs"]
mod tests;
