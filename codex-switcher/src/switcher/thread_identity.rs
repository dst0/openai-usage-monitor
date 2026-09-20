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

/// Navigates ChatGPT desktop application directly to a specific thread URL in the background without stealing focus.
pub fn open_thread_in_codex(thread_id: &str) {
    let clean = clean_thread_id(thread_id);
    if clean.is_empty() {
        return;
    }
    crate::runtime_print!("🧭 Opening thread '{}' in ChatGPT (background)...", clean);
    let _ = Command::new("/usr/bin/open")
        .args([
            "-g",
            "-a",
            "/Applications/ChatGPT.app",
            &format!("codex://threads/{}", clean),
        ])
        .status();
}

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
