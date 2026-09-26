use super::copy_deeplink_keymap_service::CopyDeeplinkKeymapService;
use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_process_validation_service::WindowProcessValidationService;
use std::path::PathBuf;

const OPT_IN_REQUIRED: &str = "Explicit --allow-focus-and-clipboard is required";

/// Explicit, opt-in window-task diagnostic. The native helper focuses each
/// ChatGPT window and copies its task link; this service reports only how
/// many distinct links it saw. It never restarts Desktop, changes
/// credentials, saves a snapshot, or prints, stores, or logs a task ID, and
/// nothing in restart, distribution, or recovery calls it.
pub struct WindowTaskProbeService;

impl WindowTaskProbeService {
    /// Each check runs before the next, more intrusive one. The helper is the
    /// only step that changes focus or the clipboard, and it runs last, while
    /// the switch/recovery operation lock keeps any restart or recovery
    /// dispatch from interleaving with the synthesized shortcut.
    pub fn run<Operation>(
        allow_focus_and_clipboard: bool,
        operation_lock: impl FnOnce() -> Result<Operation, String>,
        codex_home: impl FnOnce() -> PathBuf,
        desktop_pids: impl FnOnce() -> Result<Vec<u32>, String>,
        backend: impl FnOnce() -> Result<SystemWindowRestoreBackend, String>,
    ) -> Result<usize, String> {
        if !allow_focus_and_clipboard {
            return Err(OPT_IN_REQUIRED.into());
        }
        let _operation = operation_lock()?;
        CopyDeeplinkKeymapService::verify_default(&codex_home())?;
        let pids = desktop_pids()?;
        let [pid] = pids[..] else {
            return Err("Task probe requires exactly one ChatGPT main process".into());
        };
        let mut backend = backend()?;
        let process = WindowProcessValidationService::inspect(&mut backend, pid)?;
        backend.probe_selected_tasks(process)
    }

    /// The command's only output. Task IDs never reach Rust.
    pub fn summary(count: usize) -> String {
        format!(
            "Task probe: {count} ChatGPT window(s) each copied a distinct task link; task IDs are not shown. \
             Attribution of each clipboard write to its window is unverified. \
             The clipboard now holds the last copied link. No restart, credential change, or snapshot was made."
        )
    }
}

#[cfg(test)]
#[path = "window_task_probe_service.test.rs"]
mod tests;
