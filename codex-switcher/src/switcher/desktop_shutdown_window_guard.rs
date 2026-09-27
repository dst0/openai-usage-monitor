use super::codex_process_probe::codex_app_pids_checked;
use super::shutdown_window_snapshot::ShutdownWindowSnapshot;
use crate::distribution::{
    SystemWindowRestoreBackend, WindowProcessIdentity, WindowProcessValidationService,
};

/// Refuses to stop Desktop unless the exact process is unchanged and its
/// windows can come back: at most one window, or exactly the windows whose
/// selected tasks an explicitly requested window-task snapshot captured.
pub(super) struct DesktopShutdownWindowGuard;

impl DesktopShutdownWindowGuard {
    pub(super) fn validate(
        initial_pids: &[u32],
        expected: &WindowProcessIdentity,
        observed: &WindowProcessIdentity,
        window_ids: &[u32],
        current_pids: &[u32],
        captured_windows: Option<&[u32]>,
    ) -> Result<(), String> {
        if observed != expected {
            return Err("Desktop process birth identity changed before shutdown".into());
        }
        if initial_pids != [expected.pid] || current_pids != [expected.pid] {
            return Err("Desktop process set changed before shutdown".into());
        }
        match captured_windows {
            Some(captured) if captured != window_ids => Err(
                "Desktop windows changed after their tasks were captured; refusing restart".into(),
            ),
            Some(_) => Ok(()),
            None if window_ids.len() > 1 => Err(format!(
                "Desktop has {} ChatGPT windows, and a restart reopens only one window without its \
                 task; refusing restart. Rerun with --restore-window-tasks to capture and reopen \
                 each window's task (it focuses each window and briefly uses the clipboard)",
                window_ids.len()
            )),
            None => Ok(()),
        }
    }

    pub(super) fn signal_after_validation(
        snapshot: &ShutdownWindowSnapshot,
        expected: &WindowProcessIdentity,
        captured_windows: Option<&[u32]>,
        signal: impl FnOnce(u32) -> Result<(), String>,
    ) -> Result<(), String> {
        Self::validate(
            &snapshot.initial_pids,
            expected,
            &snapshot.observed,
            &snapshot.window_ids,
            &snapshot.current_pids,
            captured_windows,
        )?;
        signal(expected.pid)
    }

    pub(super) fn checked_snapshot(
        expected: &WindowProcessIdentity,
        captured_windows: Option<&[u32]>,
    ) -> Result<ShutdownWindowSnapshot, String> {
        let initial_pids = codex_app_pids_checked()?;
        if initial_pids.len() != 1 {
            return Err("Desktop shutdown requires exactly one ChatGPT main process".into());
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        let observed = WindowProcessValidationService::inspect(&mut backend, initial_pids[0])?;
        let window_ids = backend.capture_window_inventory(observed.clone())?;
        WindowProcessValidationService::confirm(&mut backend, &observed)?;
        let current_pids = codex_app_pids_checked()?;
        let snapshot = ShutdownWindowSnapshot {
            initial_pids,
            observed,
            window_ids,
            current_pids,
        };
        Self::validate(
            &snapshot.initial_pids,
            expected,
            &snapshot.observed,
            &snapshot.window_ids,
            &snapshot.current_pids,
            captured_windows,
        )?;
        Ok(snapshot)
    }

    #[cfg(test)]
    pub(super) fn snapshot_for_test(
        initial_pids: &[u32],
        observed: &WindowProcessIdentity,
        window_ids: &[u32],
        current_pids: &[u32],
    ) -> ShutdownWindowSnapshot {
        ShutdownWindowSnapshot {
            initial_pids: initial_pids.to_vec(),
            observed: observed.clone(),
            window_ids: window_ids.to_vec(),
            current_pids: current_pids.to_vec(),
        }
    }
}

#[cfg(test)]
#[path = "desktop_shutdown_window_guard.test.rs"]
mod tests;
