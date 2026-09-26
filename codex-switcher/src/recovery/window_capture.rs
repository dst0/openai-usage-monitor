use crate::{models::DesktopWindowBounds, storage};
use std::{path::PathBuf, process::Command};

pub(super) fn helper_candidates() -> Vec<PathBuf> {
    #[cfg(test)]
    crate::test_live_system::forbid("installed codex-ui-resume helper");
    let mut candidates = Vec::new();
    if let Ok(path) = std::env::current_exe() {
        if let Some(dir) = path.parent() {
            let h = dir.join("codex-ui-resume");
            if h.exists() && !candidates.contains(&h) {
                candidates.push(h);
            }
        }
    }
    if let Some(home) = dirs::home_dir() {
        let h = home.join(".local/bin/codex-ui-resume");
        if h.exists() && !candidates.contains(&h) {
            candidates.push(h);
        }
    }
    candidates
}

pub(crate) fn verify_desktop_window_passive(expected_pid: u32) -> Result<(), String> {
    for helper in helper_candidates() {
        let output = Command::new(helper)
            .args([
                "--verify-visible",
                "--expected-pid",
                &expected_pid.to_string(),
            ])
            .output()
            .map_err(|error| error.to_string())?;
        if output.status.success()
            && String::from_utf8_lossy(&output.stdout).contains("APP_VISIBLE")
        {
            crate::runtime_print!("RESTART_VISIBLE pid={expected_pid}");
            return Ok(());
        }
    }
    Err(format!(
        "Codex process {expected_pid} has no verified visible, non-minimized window"
    ))
}

pub fn desktop_window_bounds_path() -> PathBuf {
    storage::codex_home().join("desktop-window.json")
}

pub fn should_preserve_window_bounds() -> bool {
    storage::load_accounts()
        .map(|acc| acc.settings.preserve_window_bounds_on_restart)
        .unwrap_or(true)
}

pub fn get_saved_desktop_window_bounds() -> Result<Option<DesktopWindowBounds>, String> {
    let path = desktop_window_bounds_path();
    if !path.exists() {
        return Ok(None);
    }
    let data =
        std::fs::read(&path).map_err(|e| format!("Failed to read {}: {e}", path.display()))?;
    let bounds: DesktopWindowBounds = serde_json::from_slice(&data)
        .map_err(|e| format!("Failed to parse {}: {e}", path.display()))?;
    Ok(Some(bounds))
}

pub fn get_active_desktop_window_bounds() -> Result<DesktopWindowBounds, String> {
    for helper in helper_candidates() {
        let output = Command::new(helper)
            .arg("--get-window-bounds")
            .output()
            .map_err(|e| e.to_string())?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if let Ok(bounds) = serde_json::from_str::<DesktopWindowBounds>(stdout.trim()) {
                return Ok(bounds);
            }
        }
    }
    Err("Could not retrieve active desktop window bounds".into())
}

pub(crate) fn save_desktop_window_bounds(
    target_pid: Option<u32>,
) -> Result<Option<DesktopWindowBounds>, String> {
    if !should_preserve_window_bounds() {
        return Ok(None);
    }
    for helper in helper_candidates() {
        let mut cmd = Command::new(&helper);
        cmd.arg("--save-window-bounds");
        if let Some(pid) = target_pid {
            cmd.args(["--expected-pid", &pid.to_string()]);
        }
        let output = cmd.output().map_err(|e| e.to_string())?;
        if output.status.success() {
            let stdout = String::from_utf8_lossy(&output.stdout);
            if stdout.contains("WINDOW_BOUNDS_SAVED") {
                let saved = get_saved_desktop_window_bounds().ok().flatten();
                if let Some(ref b) = saved {
                    let pid_str = target_pid.map(|p| format!(" pid={p}")).unwrap_or_default();
                    let msg = format!(
                        "WINDOW_BOUNDS_SAVED x={:.1} y={:.1} w={:.1} h={:.1}{pid_str}",
                        b.x, b.y, b.width, b.height
                    );
                    crate::runtime_print!("{msg}");
                    crate::logger::log("INFO", "RECOVERY", &msg);
                }
                return Ok(saved);
            }
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        crate::logger::log(
            "WARN",
            "RECOVERY",
            &format!(
                "Failed to save window bounds via {}: stdout='{}' stderr='{}'",
                helper.display(),
                stdout.trim(),
                stderr.trim()
            ),
        );
    }
    crate::logger::log(
        "WARN",
        "RECOVERY",
        "No valid helper could save desktop window bounds",
    );
    Ok(None)
}
