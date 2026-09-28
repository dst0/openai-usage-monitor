#[cfg(test)]
use super::codex_process_probe::codex_app_pids_checked_with;
pub(super) use super::codex_process_probe::CODEX_APP_EXECUTABLE;
use super::codex_process_probe::{codex_app_pids_checked, shared_auth_activity_checked};
use super::desktop_shutdown_window_guard::DesktopShutdownWindowGuard;
use super::desktop_writer_exit_gate::DesktopWriterExitGate;
use crate::distribution::{AppStopError, WindowProcessIdentity};
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

const CODEX_EXIT_GRACE_PERIOD: Duration = Duration::from_secs(3);
const CODEX_WRITER_EXIT_GRACE_PERIOD: Duration = Duration::from_secs(10);
const CODEX_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(super) fn codex_app_pids() -> Vec<u32> {
    codex_app_pids_checked().unwrap_or_default()
}

pub(crate) fn current_codex_app_pids() -> Vec<u32> {
    codex_app_pids()
}

pub(crate) fn current_codex_app_pids_checked() -> Result<Vec<u32>, String> {
    codex_app_pids_checked()
}

pub(crate) fn is_codex_app_running_checked() -> Result<bool, String> {
    Ok(!codex_app_pids_checked()?.is_empty())
}

pub(crate) fn is_shared_auth_active_checked() -> Result<bool, String> {
    shared_auth_activity_checked()
}

fn desktop_writers_running_with(
    captured: impl FnOnce() -> Result<bool, String>,
    all_bundled: impl FnOnce() -> Result<bool, String>,
) -> Result<bool, String> {
    let captured_running = captured()?;
    let bundled_running = all_bundled()?;
    Ok(captured_running || bundled_running)
}

/// Checks the exact process and its windows without signalling it. Pass the
/// window IDs of an explicitly requested window-task snapshot, or `None`.
pub(crate) fn preflight_shutdown_windows(
    expected: &WindowProcessIdentity,
    captured_windows: Option<&[u32]>,
) -> Result<(), String> {
    DesktopShutdownWindowGuard::checked_snapshot(expected, captured_windows).map(|_| ())
}

pub(crate) fn stop_codex_app_gracefully(
    expected: &WindowProcessIdentity,
    captured_windows: Option<&[u32]>,
) -> Result<(), AppStopError> {
    stop_codex_app_gracefully_with(expected, captured_windows, || Ok(()))
}

pub(crate) fn stop_codex_app_gracefully_with(
    expected: &WindowProcessIdentity,
    captured_windows: Option<&[u32]>,
    verify_tasks: impl FnOnce() -> Result<(), String>,
) -> Result<(), AppStopError> {
    let writer_gate = DesktopWriterExitGate::capture(expected.pid).map_err(AppStopError::before)?;
    let snapshot = DesktopShutdownWindowGuard::checked_snapshot(expected, captured_windows)
        .map_err(AppStopError::before)?;
    DesktopShutdownWindowGuard::signal_after_validation(
        &snapshot,
        expected,
        captured_windows,
        |pid| {
            verify_then_signal(
                verify_tasks,
                || {
                    DesktopShutdownWindowGuard::checked_snapshot(expected, captured_windows)
                        .map(|_| ())
                },
                || {
                    let pid = i32::try_from(pid).map_err(|_| "Desktop PID is invalid")?;
                    // Chromium flushes SQLite and WAL on SIGTERM, avoiding
                    // the GUI beforeunload prompt.
                    if unsafe { libc::kill(pid, libc::SIGTERM) } != 0 {
                        return Err(format!(
                            "Could not signal the verified ChatGPT process: {}",
                            std::io::Error::last_os_error()
                        ));
                    }
                    Ok(())
                },
            )
        },
    )
    .map_err(AppStopError::before)?;

    // 2. Wait up to 3 seconds for graceful process exit
    let exited = wait_for_app_exit_with(
        is_codex_app_running_checked,
        CODEX_EXIT_GRACE_PERIOD,
        CODEX_EXIT_POLL_INTERVAL,
    )
    .map_err(AppStopError::after)?;

    if !exited {
        return Err(AppStopError::after(
            "ChatGPT main process did not exit gracefully; refusing to replace credentials",
        ));
    }

    let writers_exited = wait_for_app_exit_with(
        || {
            desktop_writers_running_with(
                || writer_gate.writers_running_checked(),
                is_shared_auth_active_checked,
            )
        },
        CODEX_WRITER_EXIT_GRACE_PERIOD,
        CODEX_EXIT_POLL_INTERVAL,
    )
    .map_err(AppStopError::after)?;
    if !writers_exited {
        return Err(AppStopError::after(
            "Desktop app-server remained alive after ChatGPT exited; refusing to replace credentials",
        ));
    }

    // Cooldown: allow LaunchServices, loginwindow, and Chromium auxiliary helpers
    // to cleanly deregister the old app ASN before any relaunch attempt.
    sleep(Duration::from_millis(600));

    Ok(())
}

fn verify_then_signal(
    verify_tasks: impl FnOnce() -> Result<(), String>,
    verify_windows: impl FnOnce() -> Result<(), String>,
    signal: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    verify_tasks()?;
    verify_windows()?;
    signal()
}

pub(crate) fn launch_codex_app() -> Result<Vec<u32>, String> {
    if is_shared_auth_active_checked()? {
        return Err("A Desktop credential writer is still running before launch".into());
    }
    let existing = codex_app_pids_checked()?;
    if !existing.is_empty() {
        return Err(format!(
            "Refusing to claim a new Codex launch while these main processes already exist: {existing:?}"
        ));
    }
    // `-n` avoids LaunchServices coalescing this request into the just-terminated
    // application registration. A launch succeeds only when a new exact main
    // process appears and remains alive across a short settling interval.
    for attempt in 1..=3 {
        let before_attempt = codex_app_pids_checked()?;
        if !before_attempt.is_empty() {
            return Err(format!(
                "Codex main process appeared outside the verified launch attempt: {before_attempt:?}"
            ));
        }
        let status = Command::new("/usr/bin/open")
            .env_remove("CODEX_RESTART_WORKER")
            .env_remove("CODEX_RESTART_OPERATION")
            .env_remove("CODEX_PRIMARY_THREAD")
            .args(["-g", "-n", "-a", "/Applications/ChatGPT.app"])
            .status()
            .map_err(|error| format!("Codex could not be launched: {error}"))?;

        if !status.success() {
            if attempt == 3 {
                return Err("Codex could not be launched".to_string());
            }
            sleep(Duration::from_millis(500));
            continue;
        }

        let deadline = Instant::now() + Duration::from_secs(15);
        while Instant::now() < deadline {
            let launched = codex_app_pids_checked()?;
            if !launched.is_empty() {
                if launched.len() != 1 {
                    return Err(format!(
                        "Codex launch created multiple main processes: {launched:?}"
                    ));
                }
                sleep(Duration::from_secs(2));
                let settled = codex_app_pids_checked()?;
                if settled == launched {
                    return Ok(launched);
                }
                if settled.is_empty() {
                    break;
                }
                return Err(format!(
                    "Codex main process changed during launch settling: {launched:?} -> {settled:?}"
                ));
            }
            sleep(Duration::from_millis(200));
        }

        crate::runtime_print!(
            "⚠️ Codex app did not appear after attempt {}, retrying launch...",
            attempt
        );
        sleep(Duration::from_millis(500));
    }

    Err("Codex app launch was requested, but no stable main process appeared".to_string())
}

pub(super) fn wait_for_app_exit_with<F>(
    mut is_running: F,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<bool, String>
where
    F: FnMut() -> Result<bool, String>,
{
    let deadline = Instant::now() + timeout;
    loop {
        if !is_running()? {
            return Ok(true);
        }
        if Instant::now() >= deadline {
            return Ok(false);
        }
        sleep(poll_interval);
    }
}

fn escape_applescript(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

pub fn send_macos_notification(title: &str, message: &str) {
    if let Some(home) = std::env::var_os("HOME") {
        let notifier = std::path::PathBuf::from(home)
            .join("Applications/Codex Notifier.app/Contents/MacOS/notify");
        if notifier.exists() {
            if let Ok(status) = Command::new(&notifier)
                .arg("Codex Switcher")
                .arg(message)
                .arg(title)
                .status()
            {
                if status.success() {
                    return;
                }
            }
        }
    }

    let script = format!(
        "display notification \"{}\" with title \"Codex Switcher\" subtitle \"{}\"",
        escape_applescript(message),
        escape_applescript(title)
    );
    let _ = Command::new("osascript").arg("-e").arg(script).output();
}

#[cfg(test)]
#[path = "codex_app_lifecycle.test.rs"]
mod tests;
