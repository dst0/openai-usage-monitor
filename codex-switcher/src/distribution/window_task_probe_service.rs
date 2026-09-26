use super::copy_deeplink_keymap_service::CopyDeeplinkKeymapService;
use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_process_validation_service::WindowProcessValidationService;
use std::path::PathBuf;

const OPT_IN_REQUIRED: &str = "Explicit --allow-focus-and-clipboard is required";
const NOT_DESKTOP_HOME: &str =
    "The task probe needs the Codex home ChatGPT uses (~/.codex); unset CODEX_HOME and retry";
const KEYMAP_CHANGED: &str = "ChatGPT keybindings changed during the task probe; its result is void, and a synthesized Cmd+Opt+L may have run another command";

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
        desktop_codex_home: impl FnOnce() -> Result<PathBuf, String>,
        operation_lock: impl FnOnce() -> Result<Operation, String>,
        desktop_pids: impl FnOnce() -> Result<Vec<u32>, String>,
        backend: impl FnOnce() -> Result<SystemWindowRestoreBackend, String>,
    ) -> Result<usize, String> {
        if !allow_focus_and_clipboard {
            return Err(OPT_IN_REQUIRED.into());
        }
        let home = desktop_codex_home()?;
        let _operation = operation_lock()?;
        let keymap = CopyDeeplinkKeymapService::verify_default(&home)?;
        let pids = desktop_pids()?;
        let [pid] = pids[..] else {
            return Err("Task probe requires exactly one ChatGPT main process".into());
        };
        let mut backend = backend()?;
        let process = WindowProcessValidationService::inspect(&mut backend, pid)?;
        let result = backend.probe_selected_tasks(process);
        // ChatGPT re-reads its keymap whenever one of its windows gains
        // focus, which the probe itself causes; an edit meanwhile voids it.
        if CopyDeeplinkKeymapService::verify_default(&home) != Ok(keymap) {
            return Err(KEYMAP_CHANGED.into());
        }
        result
    }

    /// The Codex home whose keymap ChatGPT applies. ChatGPT reads
    /// `CODEX_HOME` from its own environment, which is normally unset, so a
    /// CLI home elsewhere could hide that keymap and would not share the
    /// daemon's operation lock.
    pub fn desktop_codex_home(
        configured: PathBuf,
        user_home: Option<PathBuf>,
    ) -> Result<PathBuf, String> {
        let default = user_home
            .map(|home| home.join(".codex"))
            .ok_or(NOT_DESKTOP_HOME)?;
        let same = match (configured.canonicalize(), default.canonicalize()) {
            (Ok(configured), Ok(default)) => configured == default,
            _ => configured == default,
        };
        if same {
            Ok(configured)
        } else {
            Err(NOT_DESKTOP_HOME.into())
        }
    }

    /// The command's only output. Task IDs never reach Rust.
    pub fn summary(count: usize) -> String {
        format!(
            "Task probe: {count} ChatGPT window(s) each copied a distinct task link; task IDs are not shown. \
             Attribution of each clipboard write to its window is unverified. \
             The clipboard now holds the last copied link, which clipboard history or Universal Clipboard may keep. \
             No restart, credential change, or snapshot was made."
        )
    }
}

#[cfg(test)]
#[path = "window_task_probe_service.test.rs"]
mod tests;
