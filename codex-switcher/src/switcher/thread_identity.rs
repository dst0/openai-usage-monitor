use std::process::Command;

/// Strips URL schemes like codex://threads/ or chatgpt://threads/ to return a bare thread UUID.
pub fn clean_thread_id(raw: &str) -> String {
    let trimmed = raw.trim();
    if let Some(stripped) = trimmed.strip_prefix("codex://threads/") {
        stripped.trim_matches('/').to_string()
    } else if let Some(stripped) = trimmed.strip_prefix("chatgpt://threads/") {
        stripped.trim_matches('/').to_string()
    } else {
        trimmed.to_string()
    }
}

fn thread_open_command(thread_id: &str, foreground: bool) -> Command {
    let mut command = Command::new("/usr/bin/open");
    if !foreground {
        command.arg("-g");
    }
    command.args(["-a", "/Applications/ChatGPT.app"]);
    command.arg(format!("codex://threads/{thread_id}"));
    command
}

/// Foreground navigation gives Desktop a chance to mount a cold task. IPC
/// owner discovery is still required before sending any recovery request.
pub fn open_thread_in_codex(thread_id: &str) -> Result<(), String> {
    open_thread(thread_id, true)
}

/// Requests background delivery for a repeated URL. Desktop may still
/// foreground its window while mounting the task.
pub fn retry_thread_link_in_background(thread_id: &str) -> Result<(), String> {
    open_thread(thread_id, false)
}

/// Uses the exact installed ChatGPT bundle rather than the default URL
/// handler. A successful launch is only delivery, never proof of an owner.
pub fn retry_thread_link_natively_in_background(thread_id: &str) -> Result<(), String> {
    let clean = clean_thread_id(thread_id);
    if !is_valid_thread_id(&clean) {
        return Err("Invalid thread ID for ChatGPT navigation".into());
    }
    super::pinned_thread_link_launch_spec::retry(&clean)
}

fn open_thread(thread_id: &str, foreground: bool) -> Result<(), String> {
    #[cfg(test)]
    crate::test_live_system::forbid("ChatGPT task link (/usr/bin/open)");
    let clean = clean_thread_id(thread_id);
    if !is_valid_thread_id(&clean) {
        return Err("Invalid thread ID for ChatGPT navigation".into());
    }
    crate::runtime_print!("🧭 Opening thread '{}' in ChatGPT...", clean);
    let status = thread_open_command(&clean, foreground)
        .status()
        .map_err(|error| format!("Could not launch ChatGPT task link: {error}"))?;
    if !status.success() {
        return Err(format!("ChatGPT task link exited with {status}"));
    }
    Ok(())
}

#[cfg(test)]
#[path = "thread_identity.test.rs"]
mod tests;

/// Retrieves the most recently updated unarchived threads from state_5.sqlite.
pub fn get_most_recent_threads(codex_home: &std::path::Path, limit: usize) -> Vec<String> {
    let state_sqlite = codex_home.join("state_5.sqlite");
    if !state_sqlite.exists() {
        return Vec::new();
    }
    let query = format!(
        "SELECT id FROM threads WHERE archived = 0 AND (thread_source IS NULL OR thread_source != 'subagent') ORDER BY updated_at DESC LIMIT {};",
        limit
    );
    if let Ok(output) = Command::new("/usr/bin/sqlite3")
        .arg(state_sqlite.to_str().unwrap_or(""))
        .arg(&query)
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout)
                .lines()
                .map(|l| l.trim().to_string())
                .filter(|s| !s.is_empty())
                .collect();
        }
    }
    Vec::new()
}

/// Helper to check if a thread is a user-level thread (not a spawned sub-agent)
pub fn is_user_thread(codex_home: &std::path::Path, thread_id: &str) -> bool {
    if !is_valid_thread_id(thread_id) {
        return false;
    }
    let state_sqlite = codex_home.join("state_5.sqlite");
    if !state_sqlite.exists() {
        return false;
    }
    let query = format!(
        "SELECT 1 FROM threads WHERE id = '{}' AND archived = 0 AND (thread_source IS NULL OR thread_source != 'subagent') LIMIT 1;",
        thread_id
    );
    if let Ok(output) = Command::new("/usr/bin/sqlite3")
        .args(["-batch", "-cmd", ".timeout 3000"])
        .arg(state_sqlite.to_str().unwrap_or(""))
        .arg(&query)
        .output()
    {
        if output.status.success() {
            return String::from_utf8_lossy(&output.stdout).trim() == "1";
        }
    }
    // Detection must fail closed. A transient SQLite failure previously let
    // subagent lock files into a restart manifest, guaranteeing a false
    // recovery failure after the old Desktop process had already exited.
    false
}

fn is_valid_thread_id(thread_id: &str) -> bool {
    thread_id.len() == 36
        && thread_id.bytes().enumerate().all(|(index, byte)| {
            if matches!(index, 8 | 13 | 18 | 23) {
                byte == b'-'
            } else {
                byte.is_ascii_hexdigit()
            }
        })
}
