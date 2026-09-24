use super::window_capture::verify_desktop_window_passive;
use super::window_capture::{
    get_saved_desktop_window_bounds, helper_candidates, should_preserve_window_bounds,
};
use crate::{models::DesktopWindowBounds, switcher};
use std::{
    process::Command,
    thread::sleep,
    time::{Duration, Instant},
};
pub(crate) const DESKTOP_STABILITY_WINDOW: Duration = Duration::from_secs(3);

pub(super) fn restore_desktop_window_bounds_osascript(
    expected_pid: u32,
    bounds: &DesktopWindowBounds,
) -> bool {
    let script = format!(
        r#"tell application "System Events"
set targetProc to missing value
try
set targetProc to (first process whose unix id is {expected_pid})
end try
if targetProc is not missing value then
tell targetProc
set matched to missing value
repeat with w in windows
try
set subr to subrole of w
set sz to size of w
if subr is "AXStandardWindow" and (item 1 of sz >= 300 and item 2 of sz >= 250) then
set matched to w
exit repeat
end if
end try
end repeat
if matched is not missing value then
set position of matched to {{{x}, {y}}}
delay 0.15
set size of matched to {{{w}, {h}}}
delay 0.1
set position of matched to {{{x}, {y}}}
set pos to position of matched
set sz to size of matched
set actualPid to unix id
return ((item 1 of pos as integer) as text) & " " & ((item 2 of pos as integer) as text) & " " & ((item 1 of sz as integer) as text) & " " & ((item 2 of sz as integer) as text) & " " & (actualPid as text)
end if
end tell
end if
error "NO_WINDOW_FOUND"
end tell"#,
        expected_pid = expected_pid,
        x = bounds.x.round() as i64,
        y = bounds.y.round() as i64,
        w = bounds.width.round() as i64,
        h = bounds.height.round() as i64,
    );

    match Command::new("/usr/bin/osascript")
        .arg("-e")
        .arg(&script)
        .output()
    {
        Ok(out) if out.status.success() => {
            let stdout = String::from_utf8_lossy(&out.stdout);
            let parts: Vec<&str> = stdout.split_whitespace().collect();
            if parts.len() == 5 {
                let line = format!(
                    "WINDOW_BOUNDS_RESTORED x={} y={} width={} height={} pid={}",
                    parts[0], parts[1], parts[2], parts[3], parts[4]
                );
                crate::runtime_print!("{line}");
                crate::logger::log("INFO", "RECOVERY", &line);
                return true;
            }
            false
        }
        _ => false,
    }
}

pub(crate) fn restore_desktop_window_bounds(expected_pid: u32) -> Result<(), String> {
    if !should_preserve_window_bounds() {
        return Ok(());
    }
    for helper in helper_candidates() {
        let output = Command::new(&helper)
            .args([
                "--restore-window-bounds",
                "--expected-pid",
                &expected_pid.to_string(),
            ])
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("WINDOW_BOUNDS_RESTORED") {
                let line = stdout
                    .lines()
                    .find(|l| l.contains("WINDOW_BOUNDS_RESTORED"))
                    .unwrap_or("WINDOW_BOUNDS_RESTORED");
                crate::runtime_print!("{line}");
                crate::logger::log("INFO", "RECOVERY", line);
                return Ok(());
            } else if stdout.contains("NO_BOUNDS_SAVED") {
                crate::logger::log(
                    "INFO",
                    "RECOVERY",
                    &format!(
                        "WINDOW_BOUNDS_RESTORE skipped: no saved bounds for pid={expected_pid}"
                    ),
                );
                return Ok(());
            } else {
                crate::logger::log(
                    "WARN",
                    "RECOVERY",
                    &format!(
                        "WINDOW_BOUNDS_RESTORE unexpected helper output: '{}'",
                        stdout.trim()
                    ),
                );
            }
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        crate::logger::log(
            "WARN",
            "RECOVERY",
            &format!(
                "Failed to restore window bounds via {}: stdout='{}' stderr='{}'",
                helper.display(),
                stdout.trim(),
                stderr.trim()
            ),
        );
    }

    // Direct AppleScript fallback: runs System Events via /usr/bin/osascript
    if let Ok(Some(saved)) = get_saved_desktop_window_bounds() {
        if restore_desktop_window_bounds_osascript(expected_pid, &saved) {
            return Ok(());
        }
    }

    crate::logger::log(
        "WARN",
        "RECOVERY",
        &format!("No valid helper or AppleScript fallback could restore desktop window bounds for pid={expected_pid}"),
    );
    Ok(())
}

pub(crate) fn verify_desktop_stable(
    expected_pids: &[u32],
    require_window: bool,
) -> Result<(), String> {
    if expected_pids.len() != 1 {
        return Err(format!(
            "Codex launch must produce exactly one main process, got {expected_pids:?}"
        ));
    }
    let deadline = Instant::now() + DESKTOP_STABILITY_WINDOW;
    loop {
        if switcher::current_codex_app_pids() != expected_pids {
            return Err(
                "Codex main process changed or exited during recovery stabilization".into(),
            );
        }
        if Instant::now() >= deadline {
            break;
        }
        sleep(Duration::from_millis(250));
    }
    if require_window {
        verify_desktop_window_passive(expected_pids[0])?;
    }
    crate::runtime_print!(
        "RESTART_STABLE pids={expected_pids:?} observation_secs={}",
        DESKTOP_STABILITY_WINDOW.as_secs()
    );
    Ok(())
}
