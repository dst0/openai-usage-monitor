use super::copy_deeplink_keymap_service::CopyDeeplinkKeymapService;
use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_process_validation_service::WindowProcessValidationService;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_command::WindowTaskCommand;
use super::window_task_helper_client::WindowTaskHelperClient;
use super::window_task_probe_validation_service::WindowTaskProbeValidationService;
use super::window_task_report::WindowTaskReport;
use super::window_task_session_validation_service::WindowTaskSessionValidationService;
use std::ffi::{CStr, OsStr};
use std::os::unix::ffi::OsStrExt;
use std::path::PathBuf;

const OPT_IN_REQUIRED: &str = "Explicit --allow-focus-and-clipboard is required";
const NOT_DESKTOP_HOME: &str =
    "The task probe needs the Codex home ChatGPT uses (~/.codex); unset CODEX_HOME and retry";
const KEYMAP_CHANGED: &str = "ChatGPT keybindings changed during the window-task command; its result is void, and a synthesized Cmd+Opt+L may have run another command";
/// Added to any failure the helper did not name: it may have happened after
/// the helper began focusing windows.
const POSSIBLE_VISIBLE_CHANGE: &str =
    "ChatGPT windows may have been focused and the clipboard may hold a copied task link";

/// Explicit, opt-in window-task diagnostics. The probe focuses each ChatGPT
/// window and copies its task link, reporting only how many distinct links
/// it saw. The rehearsal additionally opens one new window per original,
/// sends it the original's task link, checks it, and closes it again. Both
/// put the clipboard back when nothing else wrote to it. Neither restarts
/// Desktop, changes credentials, saves a snapshot, or prints, stores, or logs
/// a task ID, and nothing in restart, distribution, or recovery calls them.
pub struct WindowTaskProbeService;

impl WindowTaskProbeService {
    pub fn run<Operation>(
        allow_focus_and_clipboard: bool,
        desktop_codex_home: impl FnOnce() -> Result<PathBuf, String>,
        operation_lock: impl FnOnce() -> Result<Operation, String>,
        desktop_pids: impl FnOnce() -> Result<Vec<u32>, String>,
        backend: impl FnOnce() -> Result<SystemWindowRestoreBackend, String>,
    ) -> Result<WindowTaskReport, String> {
        Self::guarded(
            allow_focus_and_clipboard,
            desktop_codex_home,
            operation_lock,
            desktop_pids,
            backend,
            |backend, process, windows| {
                let response = WindowTaskHelperClient::new(backend).run(
                    WindowTaskCommand::Probe,
                    &process,
                    None,
                    WindowTaskCommand::Probe.timeout(windows),
                )?;
                WindowTaskProbeValidationService::parse(&response, &process)
            },
        )
    }

    pub fn rehearse<Operation>(
        allow_focus_and_clipboard: bool,
        desktop_codex_home: impl FnOnce() -> Result<PathBuf, String>,
        operation_lock: impl FnOnce() -> Result<Operation, String>,
        desktop_pids: impl FnOnce() -> Result<Vec<u32>, String>,
        backend: impl FnOnce() -> Result<SystemWindowRestoreBackend, String>,
    ) -> Result<WindowTaskReport, String> {
        Self::guarded(
            allow_focus_and_clipboard,
            desktop_codex_home,
            operation_lock,
            desktop_pids,
            backend,
            |backend, process, windows| {
                let response = WindowTaskHelperClient::new(backend).run(
                    WindowTaskCommand::Rehearse,
                    &process,
                    None,
                    WindowTaskCommand::Rehearse.timeout(windows),
                )?;
                WindowTaskSessionValidationService::rehearsal(&response, &process)
            },
        )
    }

    /// Each check runs before the next, more intrusive one. The helper is the
    /// only step that changes focus or the clipboard, and it runs last, while
    /// the switch/recovery operation lock keeps any restart or recovery
    /// dispatch from interleaving with the synthesized shortcut.
    fn guarded<Operation, T>(
        allow_focus_and_clipboard: bool,
        desktop_codex_home: impl FnOnce() -> Result<PathBuf, String>,
        operation_lock: impl FnOnce() -> Result<Operation, String>,
        desktop_pids: impl FnOnce() -> Result<Vec<u32>, String>,
        backend: impl FnOnce() -> Result<SystemWindowRestoreBackend, String>,
        command: impl FnOnce(&SystemWindowRestoreBackend, ProcessIdentity, usize) -> Result<T, String>,
    ) -> Result<T, String> {
        if !allow_focus_and_clipboard {
            return Err(OPT_IN_REQUIRED.into());
        }
        let home = desktop_codex_home()?;
        let _operation = operation_lock()?;
        let keymap = CopyDeeplinkKeymapService::verify_copy_binding(&home)?;
        let pids = desktop_pids()?;
        let [pid] = pids[..] else {
            return Err("Window-task commands require exactly one ChatGPT main process".into());
        };
        let mut backend = backend()?;
        let process = WindowProcessValidationService::inspect(&mut backend, pid)?;
        // The read-only inventory sizes the helper's deadline.
        let windows = backend.capture_window_inventory(process.clone())?.len();
        let result = command(&backend, process, windows)
            .map_err(|error| Self::with_visible_change_caveat(&error));
        // ChatGPT re-reads its keymap whenever one of its windows gains
        // focus, which the probe itself causes; an edit meanwhile voids it.
        if CopyDeeplinkKeymapService::verify_copy_binding(&home) != Ok(keymap) {
            return Err(KEYMAP_CHANGED.into());
        }
        result
    }

    /// A named failure already says whether it followed a focus change; any
    /// other failure after the helper started might have, so it says so.
    pub(super) fn with_visible_change_caveat(error: &str) -> String {
        if WindowTaskCommand::is_named_failure(error) {
            error.to_string()
        } else {
            format!("{error}; {POSSIBLE_VISIBLE_CHANGE}")
        }
    }

    /// The account's home directory from the user database. `$HOME` can be
    /// pointed anywhere by the caller, while ChatGPT, started by launchd,
    /// gets the account's home.
    pub fn account_home() -> Option<PathBuf> {
        let mut buffer = vec![0 as libc::c_char; 16 * 1024];
        // SAFETY: `passwd` is plain C data; getpwuid_r fills it and points
        // its strings into `buffer`, which outlives every use below.
        let mut entry: libc::passwd = unsafe { std::mem::zeroed() };
        let mut found: *mut libc::passwd = std::ptr::null_mut();
        let status = unsafe {
            libc::getpwuid_r(
                libc::getuid(),
                &mut entry,
                buffer.as_mut_ptr(),
                buffer.len(),
                &mut found,
            )
        };
        if status != 0 || found.is_null() || entry.pw_dir.is_null() {
            return None;
        }
        // SAFETY: checked non-null above; getpwuid_r NUL-terminates it.
        let directory = unsafe { CStr::from_ptr(entry.pw_dir) };
        Some(PathBuf::from(OsStr::from_bytes(directory.to_bytes())))
    }

    /// The Codex home whose keymap ChatGPT applies. ChatGPT reads
    /// `CODEX_HOME` from its own environment, which is normally unset, so a
    /// CLI home elsewhere could hide that keymap and would not share the
    /// daemon's operation lock. A ChatGPT started with its own `CODEX_HOME`
    /// (for example through `launchctl setenv`) is not detected.
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

    /// The probe command's only output. Task IDs never reach Rust.
    pub fn summary(report: WindowTaskReport) -> String {
        format!(
            "Task probe: {} ChatGPT window(s) each copied a distinct task link; task IDs are not shown. \
             Attribution of each clipboard write to its window is unverified. {} \
             No restart, credential change, or snapshot was made.",
            report.windows,
            Self::clipboard_note(report.clipboard_restored)
        )
    }

    /// The rehearsal command's only output. Task IDs never reach Rust.
    pub fn rehearsal_summary(report: WindowTaskReport) -> String {
        format!(
            "Task restore rehearsal: {} of {} ChatGPT window(s) had their task reopened in a new window \
             by New Window plus a task link, verified by Copy deeplink; the originals kept their tasks \
             and the extra windows were closed. Task IDs are not shown. {} \
             No restart, credential change, or snapshot was made.",
            report.verified,
            report.windows,
            Self::clipboard_note(report.clipboard_restored)
        )
    }

    fn clipboard_note(restored: bool) -> &'static str {
        if restored {
            "The previous clipboard contents were put back; clipboard history or Universal Clipboard may still have seen the copied links."
        } else {
            "The clipboard was not put back (it was private, too large, or changed by another app), so it may hold the last copied link."
        }
    }
}

#[cfg(test)]
#[path = "window_task_probe_service.test.rs"]
mod tests;
