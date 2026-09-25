use super::recovery_banner::RecoveryBanner;
use crate::distribution::system_app_lifecycle::optional_banner_capture_failure;
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
        let process = WindowProcessValidationService::inspect(backend, pid)?;
        if ids.is_empty() {
            return Ok(Self::without_window(process));
        }
        match backend.capture_banner_window(process.clone()) {
            Ok(placement) => {
                WindowProcessValidationService::confirm(backend, &process)?;
                Self::start_without_restore(operation_id, ids, reason, placement).or_else(|error| {
                    if error != "Recovery banner helper did not confirm a visible panel" {
                        return Err(error);
                    }
                    crate::logger::log(
                        "WARN",
                        "RECOVERY",
                        "RECOVERY_BANNER_UNAVAILABLE: native panel did not become visible",
                    );
                    Ok(Self::without_window(process))
                })
            }
            Err(error) if optional_banner_capture_failure(&error) => {
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
