use super::copy_deeplink_keymap_service::CopyDeeplinkKeymapService;
use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_process_validation_service::WindowProcessValidationService;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_command::WindowTaskCommand;
use super::window_task_helper_client::WindowTaskHelperClient;
use super::window_task_probe_service::WindowTaskProbeService;
use super::window_task_restore_report::WindowTaskRestoreReport;
use super::window_task_session_validation_service::WindowTaskSessionValidationService;
use super::window_task_snapshot::WindowTaskSnapshot;
use std::path::PathBuf;
use std::time::SystemTime;

const KEYMAP_CHANGED: &str =
    "ChatGPT keybindings changed while window tasks were being restored; refusing to synthesize Copy deeplink";
const WINDOWS_CHANGED: &str =
    "Desktop windows changed while their tasks were being captured; refusing restart";

/// Carries each ChatGPT window's selected task through one restart that the
/// user explicitly asked to restore windows for (`--restore-window-tasks`).
///
/// Capture happens before shutdown and fails closed, so the multiwindow
/// shutdown guard still refuses the restart when any window's task cannot be
/// read. After the relaunch, failures are recorded and reported but never
/// stop credential, checkpoint, or recovery work: the tasks themselves are
/// unchanged, only their windows may be missing. Task IDs stay in memory.
pub struct WindowTaskRestartSession {
    home: PathBuf,
    keymap: Option<SystemTime>,
    snapshot: WindowTaskSnapshot,
    failures: Vec<String>,
}

impl WindowTaskRestartSession {
    /// Before shutdown, under the caller's operation lock. `inventory` is the
    /// exact window list the shutdown guard just read for `expected`.
    pub fn capture(
        expected: &ProcessIdentity,
        desktop_codex_home: Result<PathBuf, String>,
        backend: &SystemWindowRestoreBackend,
        inventory: &[u32],
    ) -> Result<Self, String> {
        let home = desktop_codex_home?;
        let keymap = CopyDeeplinkKeymapService::verify_copy_binding(&home)?;
        let snapshot = WindowTaskHelperClient::new(backend)
            .run(WindowTaskCommand::Snapshot, expected, None)
            .and_then(|response| WindowTaskSessionValidationService::snapshot(&response, expected))
            .map_err(|error| WindowTaskProbeService::with_visible_change_caveat(&error))?;
        if CopyDeeplinkKeymapService::verify_copy_binding(&home) != Ok(keymap) {
            return Err(KEYMAP_CHANGED.into());
        }
        if snapshot.window_ids() != inventory {
            return Err(WINDOWS_CHANGED.into());
        }
        Ok(Self {
            home,
            keymap,
            snapshot,
            failures: Vec::new(),
        })
    }

    /// The windows the shutdown guard may accept, sorted.
    pub fn window_ids(&self) -> Vec<u32> {
        self.snapshot.window_ids()
    }

    pub fn window_count(&self) -> usize {
        self.snapshot.windows.len()
    }

    /// Once after the relaunch, and again after recovery, which may send its
    /// own task links. `ready` waits until Desktop can mount a task (its IPC
    /// router answers); without it a task link can be dropped.
    pub fn restore(
        &mut self,
        pid: u32,
        backend: &mut SystemWindowRestoreBackend,
        ready: impl FnOnce() -> Result<(), String>,
        phase: &str,
    ) {
        let result = ready().and_then(|()| self.restore_now(pid, backend));
        match result {
            Ok(report) if report.is_complete() => {}
            Ok(report) => self.failures.push(format!(
                "{phase}: {} of {} window(s) verified on their task",
                report.verified_count(),
                report.verified.len()
            )),
            Err(error) => self.failures.push(format!("{phase}: {error}")),
        }
    }

    /// Records a step that could not run, such as a missing helper.
    pub fn record_failure(&mut self, phase: &str, error: &str) {
        self.failures.push(format!("{phase}: {error}"));
    }

    /// The outcome once the restart is over.
    pub fn finish(self) -> Result<(), String> {
        if self.failures.is_empty() {
            return Ok(());
        }
        Err(format!(
            "Window tasks were not fully restored ({}); every task is unchanged and can be reopened from the sidebar",
            self.failures.join("; ")
        ))
    }

    fn restore_now(
        &self,
        pid: u32,
        backend: &mut SystemWindowRestoreBackend,
    ) -> Result<WindowTaskRestoreReport, String> {
        if CopyDeeplinkKeymapService::verify_copy_binding(&self.home) != Ok(self.keymap) {
            return Err(KEYMAP_CHANGED.into());
        }
        let process = WindowProcessValidationService::inspect(backend, pid)?;
        let plan = serde_json::to_vec(&self.snapshot.restore_plan())
            .map_err(|_| "Window task restore plan could not be encoded".to_string())?;
        let report = WindowTaskHelperClient::new(backend)
            .run(WindowTaskCommand::Restore, &process, Some(&plan))
            .and_then(|response| {
                WindowTaskSessionValidationService::restore(
                    &response,
                    &process,
                    self.snapshot.windows.len(),
                )
            })
            .map_err(|error| WindowTaskProbeService::with_visible_change_caveat(&error))?;
        if CopyDeeplinkKeymapService::verify_copy_binding(&self.home) != Ok(self.keymap) {
            return Err(KEYMAP_CHANGED.into());
        }
        Ok(report)
    }
}

#[cfg(test)]
#[path = "window_task_restart_session.test.rs"]
mod tests;
