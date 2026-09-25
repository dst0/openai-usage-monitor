use super::recovery_banner::RecoveryBanner;
use crate::distribution::{SystemWindowRestoreBackend, WindowProcessValidationService};
use crate::switcher;

impl RecoveryBanner {
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
