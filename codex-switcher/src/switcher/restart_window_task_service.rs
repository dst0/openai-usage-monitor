use crate::distribution::{
    SystemWindowRestoreBackend, WindowProcessIdentity, WindowTaskProbeService,
    WindowTaskRestartSession,
};

/// Carries an explicit `--restore-window-tasks` request through `cxi restart`
/// and `cxi switch`. Without that request nothing here runs, and the
/// multiwindow shutdown guard refuses more than one window as before. The
/// automatic, distribution, and Monitor-app paths never make the request.
pub(super) struct RestartWindowTaskService;

impl RestartWindowTaskService {
    /// Reads every window's task before shutdown; any failure refuses the
    /// restart before credentials or checkpoints change.
    pub(super) fn capture_if_requested(
        requested: bool,
        expected: &WindowProcessIdentity,
    ) -> Result<Option<WindowTaskRestartSession>, String> {
        if !requested {
            return Ok(None);
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        let inventory = backend.capture_window_inventory(expected.clone())?;
        let home = WindowTaskProbeService::desktop_codex_home(
            crate::storage::codex_home(),
            WindowTaskProbeService::account_home(),
        );
        let session = WindowTaskRestartSession::capture(expected, home, &backend, &inventory)?;
        crate::runtime_print!("WINDOW_TASKS_CAPTURED windows={}", session.window_count());
        Ok(Some(session))
    }

    /// The window IDs the shutdown guard may accept for this restart.
    pub(super) fn captured_windows(session: &Option<WindowTaskRestartSession>) -> Option<Vec<u32>> {
        session.as_ref().map(WindowTaskRestartSession::window_ids)
    }

    /// Reopens each captured task in its own window of the relaunched
    /// process; records, never raises, a failure.
    pub(super) fn restore(session: &mut Option<WindowTaskRestartSession>, pid: u32, phase: &str) {
        let Some(session) = session.as_mut() else {
            return;
        };
        match SystemWindowRestoreBackend::new() {
            Ok(mut backend) => session.restore(
                pid,
                &mut backend,
                crate::recovery::wait_for_desktop_ipc,
                phase,
            ),
            Err(error) => session.record_failure(phase, &error),
        }
    }

    /// Adds the window-task outcome to a restart's other failure, if any.
    pub(super) fn append_failure(
        error: Option<String>,
        session: Option<WindowTaskRestartSession>,
    ) -> Option<String> {
        match (error, Self::finish(session)) {
            (error, Ok(())) => error,
            (None, Err(window)) => Some(window),
            (Some(error), Err(window)) => Some(format!("{error}; {window}")),
        }
    }

    pub(super) fn finish(session: Option<WindowTaskRestartSession>) -> Result<(), String> {
        let Some(session) = session else {
            return Ok(());
        };
        let count = session.window_count();
        session.finish()?;
        crate::runtime_print!("WINDOW_TASKS_RESTORED windows={count}");
        Ok(())
    }
}

#[cfg(test)]
#[path = "restart_window_task_service.test.rs"]
mod tests;
