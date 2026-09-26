use std::process::Command;

pub(super) const CODEX_APP_EXECUTABLE: &str = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";
const BUNDLED_CODEX_PREFIX: &str = "/Applications/ChatGPT.app/Contents/Resources/codex-cli/";
const LEGACY_BUNDLED_CODEX: &str = "/Applications/ChatGPT.app/Contents/Resources/codex";

pub(super) fn parse_codex_app_pids(process_list: &str) -> Result<Vec<u32>, String> {
    let pids: Vec<u32> = parse_process_lines(process_list)?
        .into_iter()
        .filter_map(|(pid, executable)| (executable == CODEX_APP_EXECUTABLE).then_some(pid))
        .collect();
    if pids.iter().any(|pid| *pid <= 1) {
        return Err("Desktop process inspection returned an invalid Desktop PID".into());
    }
    Ok(pids)
}

pub(super) fn parse_shared_auth_activity(process_list: &str) -> Result<bool, String> {
    Ok(parse_process_lines(process_list)?
        .into_iter()
        .any(|(_, executable)| {
            executable == CODEX_APP_EXECUTABLE
                || executable == LEGACY_BUNDLED_CODEX
                || executable.starts_with(BUNDLED_CODEX_PREFIX)
        }))
}

fn parse_process_lines(process_list: &str) -> Result<Vec<(u32, &str)>, String> {
    process_list
        .lines()
        .map(|line| {
            let trimmed = line.trim_start();
            let split_at = trimmed
                .find(char::is_whitespace)
                .ok_or("Desktop process inspection returned an invalid row")?;
            let (pid, executable) = trimmed.split_at(split_at);
            let pid = pid
                .parse::<u32>()
                .map_err(|_| "Desktop process inspection returned an invalid PID")?;
            if executable.trim().is_empty() {
                return Err("Desktop process inspection returned an invalid process".into());
            }
            Ok((pid, executable.trim()))
        })
        .collect()
}

pub(super) fn codex_app_pids_checked_with(
    output: impl FnOnce() -> Result<String, String>,
) -> Result<Vec<u32>, String> {
    parse_codex_app_pids(&output()?)
}

pub(super) fn codex_app_pids_checked() -> Result<Vec<u32>, String> {
    codex_app_pids_checked_with(|| ps_output_checked("pid=,comm="))
}

pub(super) fn shared_auth_activity_checked() -> Result<bool, String> {
    parse_shared_auth_activity(&ps_output_checked("pid=,comm=")?)
}

pub(super) fn desktop_process_rows_checked() -> Result<String, String> {
    ps_output_checked("pid=,ppid=,comm=")
}

fn ps_output_checked(columns: &str) -> Result<String, String> {
    let output = Command::new("/bin/ps")
        .args(["-axo", columns])
        .output()
        .map_err(|_| "Desktop process inspection could not start".to_string())?;
    if !output.status.success() {
        return Err("Desktop process inspection failed".into());
    }
    String::from_utf8(output.stdout)
        .map_err(|_| "Desktop process inspection returned invalid text".into())
}
