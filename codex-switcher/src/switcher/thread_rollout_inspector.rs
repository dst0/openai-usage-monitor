use super::ThreadRolloutState;
use std::process::Command;

/// A quota failure is eligible for an automatic reset only briefly after it
/// happened. Older failed turns may have been intentionally abandoned and
/// must never spend a reset credit.
pub const RECENT_QUOTA_WINDOW_SECS: i64 = 4 * 3600;
pub fn find_thread_rollout_path(
    codex_home: &std::path::Path,
    thread_id: &str,
) -> Option<std::path::PathBuf> {
    let state_sqlite = codex_home.join("state_5.sqlite");
    if state_sqlite.exists() {
        let query = format!(
            "SELECT rollout_path FROM threads WHERE id = '{}' LIMIT 1;",
            thread_id
        );
        if let Ok(output) = Command::new("/usr/bin/sqlite3")
            .arg(state_sqlite.to_str().unwrap_or(""))
            .arg(&query)
            .output()
        {
            if output.status.success() {
                let p_str = String::from_utf8_lossy(&output.stdout).trim().to_string();
                if !p_str.is_empty() && std::path::Path::new(&p_str).exists() {
                    return Some(std::path::PathBuf::from(p_str));
                }
            }
        }
    }

    let sessions_dir = codex_home.join("sessions");
    let mut candidates: Vec<std::path::PathBuf> = Vec::new();

    fn scan_sessions(
        dir: &std::path::Path,
        thread_id: &str,
        matches: &mut Vec<std::path::PathBuf>,
    ) {
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_dir() {
                    scan_sessions(&p, thread_id, matches);
                } else if let Some(name) = p.file_name().and_then(|n| n.to_str()) {
                    if name.contains(thread_id) && name.ends_with(".jsonl") {
                        matches.push(p);
                    }
                }
            }
        }
    }

    scan_sessions(&sessions_dir, thread_id, &mut candidates);
    candidates.sort_by(|a, b| {
        let m_a = a
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let m_b = b
            .metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        m_b.cmp(&m_a)
    });

    candidates.into_iter().next()
}

/// Reads lines from the tail of a rollout file without reading the whole file into memory.
pub fn read_rollout_tail_lines(path: &std::path::Path, max_bytes: u64) -> Vec<String> {
    use std::io::{Read, Seek, SeekFrom};
    let mut file = match std::fs::File::open(path) {
        Ok(f) => f,
        Err(_) => return Vec::new(),
    };
    let len = file.metadata().map(|m| m.len()).unwrap_or(0);
    let seek_pos = len.saturating_sub(max_bytes);
    if file.seek(SeekFrom::Start(seek_pos)).is_err() {
        return Vec::new();
    }
    let mut buf = Vec::new();
    if file.read_to_end(&mut buf).is_err() {
        return Vec::new();
    }
    let text = String::from_utf8_lossy(&buf);
    text.lines()
        .map(|l| l.trim().to_string())
        .filter(|l| !l.is_empty())
        .collect()
}

/// Inspects lines from the end of a rollout to determine the state of the latest turn.
pub fn inspect_thread_rollout_state_from_lines(lines: &[String]) -> ThreadRolloutState {
    for line in lines.iter().rev() {
        let val = match serde_json::from_str::<serde_json::Value>(line) {
            Ok(v) => v,
            Err(_) => continue, // Skip potentially truncated initial line of seek window
        };

        let payload = val.get("payload");
        let payload_type = payload.and_then(|p| p.get("type")).and_then(|t| t.as_str());
        let val_type = val.get("type").and_then(|t| t.as_str());

        // Skip post-turn completion metadata and telemetry events
        if matches!(
            payload_type,
            Some("item_completed") | Some("thread_settings_applied") | Some("token_count")
        ) || matches!(
            val_type,
            Some("token_usage_record") | Some("inter_agent_communication_metadata")
        ) {
            continue;
        }

        if payload_type == Some("task_complete") {
            if let Some(err) = payload.and_then(|p| p.get("error")) {
                if !err.is_null() {
                    let err_str = err.to_string().to_lowercase();
                    if err_str.contains("usage_limit_exceeded")
                        || err_str.contains("workspace_owner_credits_depleted")
                        || err_str.contains("out of credits")
                        || err_str.contains("credits")
                        || err_str.contains("limit")
                        || err_str.contains("quota")
                    {
                        return ThreadRolloutState::InterruptedByQuota;
                    }
                    // Non-quota error, but turn completed
                    return ThreadRolloutState::CleanCompleted;
                }
            }
            return ThreadRolloutState::CleanCompleted;
        }

        if payload_type == Some("turn_aborted") {
            return ThreadRolloutState::TurnAborted;
        }

        if matches!(
            payload_type,
            Some("task_started")
                | Some("user_message")
                | Some("agent_message")
                | Some("message")
                | Some("reasoning")
                | Some("custom_tool_call")
                | Some("custom_tool_call_output")
                | Some("function_call")
                | Some("function_call_output")
                | Some("web_search")
                | Some("file_change")
        ) {
            return ThreadRolloutState::ActiveInProgress;
        }
    }

    ThreadRolloutState::Unknown
}

/// Inspects the rollout log of a thread to determine its current state.
pub fn inspect_thread_rollout_state(
    codex_home: &std::path::Path,
    thread_id: &str,
) -> ThreadRolloutState {
    if let Some(path) = find_thread_rollout_path(codex_home, thread_id) {
        let lines = read_rollout_tail_lines(&path, 131072);
        let state = inspect_thread_rollout_state_from_lines(&lines);
        if state != ThreadRolloutState::Unknown {
            return state;
        }
        let extended_lines = read_rollout_tail_lines(&path, 524288);
        return inspect_thread_rollout_state_from_lines(&extended_lines);
    }
    ThreadRolloutState::Unknown
}
