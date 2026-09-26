use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

const CODEX_EXIT_GRACE_PERIOD: Duration = Duration::from_secs(3);
const CODEX_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(250);
pub(super) const CODEX_APP_EXECUTABLE: &str = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";

pub(super) fn parse_codex_app_pids(process_list: &str) -> Vec<u32> {
    process_list
        .lines()
        .filter_map(|line| {
            let trimmed = line.trim_start();
            let split_at = trimmed.find(char::is_whitespace)?;
            let (pid, executable) = trimmed.split_at(split_at);
            if executable.trim() == CODEX_APP_EXECUTABLE {
                pid.parse::<u32>().ok()
            } else {
                None
            }
        })
        .collect()
}

pub(super) fn codex_app_pids() -> Vec<u32> {
    let output = match Command::new("/bin/ps")
        .args(["-axo", "pid=,comm="])
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return Vec::new(),
    };

    parse_codex_app_pids(&String::from_utf8_lossy(&output.stdout))
}

pub(crate) fn current_codex_app_pids() -> Vec<u32> {
    codex_app_pids()
}

pub fn is_codex_app_running() -> bool {
    !codex_app_pids().is_empty()
}

fn signal_codex_app(signal: &str) {
    let pids = codex_app_pids();
    if pids.is_empty() {
        return;
    }

    let _ = Command::new("/bin/kill")
        .arg(signal)
        .args(pids.iter().map(u32::to_string))
        .status();
}

pub(crate) fn stop_codex_app_gracefully() -> Result<(), String> {
    // 1. Send SIGTERM to ChatGPT main process.
    // Chromium catches SIGTERM to flush SQLite databases, cookies, and WAL logs cleanly,
    // while completely bypassing the interactive GUI beforeunload ("Leave site?") prompt.
    signal_codex_app("-TERM");

    // 2. Wait up to 3 seconds for graceful process exit
    let exited = wait_for_app_exit_with(
        is_codex_app_running,
        CODEX_EXIT_GRACE_PERIOD,
        CODEX_EXIT_POLL_INTERVAL,
    );

    if !exited {
        return Err(
            "Codex did not exit gracefully; refusing to force-kill it or replace credentials"
                .into(),
        );
    }

    // Cooldown: allow LaunchServices, loginwindow, and Chromium auxiliary helpers
    // to cleanly deregister the old app ASN before any relaunch attempt.
    sleep(Duration::from_millis(600));

    Ok(())
}

pub(crate) fn launch_codex_app() -> Result<Vec<u32>, String> {
    let existing = codex_app_pids();
    if !existing.is_empty() {
        return Err(format!(
            "Refusing to claim a new Codex launch while these main processes already exist: {existing:?}"
        ));
    }
    // `-n` avoids LaunchServices coalescing this request into the just-terminated
    // application registration. A launch succeeds only when a new exact main
    // process appears and remains alive across a short settling interval.
    for attempt in 1..=3 {
        let before_attempt = codex_app_pids();
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
            let launched = codex_app_pids();
            if !launched.is_empty() {
                if launched.len() != 1 {
                    return Err(format!(
                        "Codex launch created multiple main processes: {launched:?}"
                    ));
                }
                sleep(Duration::from_secs(2));
                let settled = codex_app_pids();
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
) -> bool
where
    F: FnMut() -> bool,
{
    let deadline = Instant::now() + timeout;
    loop {
        if !is_running() {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
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
