use super::*;
use fs2::FileExt;
use std::process::Command;

/// SQLite `updated_at` also changes when Desktop merely opens a task. Only
/// the terminal rollout event can date the quota failure itself.
pub(crate) fn quota_failure_timestamp(
    codex_home: &std::path::Path,
    thread_id: &str,
) -> Option<i64> {
    let path = find_thread_rollout_path(codex_home, thread_id)?;
    let mut lines = super::thread_rollout_inspector::read_rollout_tail_lines(&path, 131072);
    if super::thread_rollout_inspector::inspect_thread_rollout_state_from_lines(&lines)
        == ThreadRolloutState::Unknown
    {
        lines = super::thread_rollout_inspector::read_rollout_tail_lines(&path, 524288);
    }
    if super::thread_rollout_inspector::inspect_thread_rollout_state_from_lines(&lines)
        != ThreadRolloutState::InterruptedByQuota
    {
        return None;
    }
    for line in lines.iter().rev() {
        let event: serde_json::Value = serde_json::from_str(line).ok()?;
        if event.pointer("/payload/type").and_then(|v| v.as_str()) == Some("task_complete") {
            let timestamp = event.get("timestamp")?.as_str()?;
            return chrono::DateTime::parse_from_rfc3339(timestamp)
                .ok()
                .map(|value| value.timestamp());
        }
    }
    None
}

pub(crate) fn recent_quota_failure(
    codex_home: &std::path::Path,
    thread_id: &str,
    now: i64,
    window_secs: i64,
) -> bool {
    quota_failure_timestamp(codex_home, thread_id)
        .and_then(|failed_at| now.checked_sub(failed_at))
        .is_some_and(|age| (0..=window_secs).contains(&age))
}

/// Returns the unix timestamp of the thread's updated_at field from state_5.sqlite.
pub fn get_thread_updated_at(codex_home: &std::path::Path, thread_id: &str) -> Option<i64> {
    let state_sqlite = codex_home.join("state_5.sqlite");
    if !state_sqlite.exists() {
        return None;
    }
    let query = format!(
        "SELECT updated_at FROM threads WHERE id = '{}' LIMIT 1;",
        thread_id
    );
    let output = Command::new("/usr/bin/sqlite3")
        .arg(state_sqlite.to_str().unwrap_or(""))
        .arg(&query)
        .output()
        .ok()?;
    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        s.parse::<i64>().ok()
    } else {
        None
    }
}

/// Finds user-owned tasks whose latest terminal rollout event is a quota failure within a custom window.
pub fn detect_quota_blocked_user_threads_since(window_secs: i64) -> Vec<String> {
    let codex_home = crate::storage::codex_home();
    let now = chrono::Utc::now().timestamp();
    detect_quota_blocked_user_threads_since_at(&codex_home, now, window_secs)
}

fn detect_quota_blocked_user_threads_since_at(
    codex_home: &std::path::Path,
    now: i64,
    window_secs: i64,
) -> Vec<String> {
    get_most_recent_threads(codex_home, 30)
        .into_iter()
        .filter(|thread_id| is_user_thread(codex_home, thread_id))
        .filter(|thread_id| recent_quota_failure(codex_home, thread_id, now, window_secs))
        .collect()
}

/// Finds only recent, unarchived, user-owned tasks whose latest terminal
/// rollout event is a quota failure. Unlike `detect_in_progress_threads`, it
/// intentionally excludes active, aborted, queued, and manifest-only tasks:
/// those are safe to recover but are not proof that a weekly reset is needed.
pub fn detect_recent_quota_blocked_user_threads() -> Vec<String> {
    detect_quota_blocked_user_threads_since(RECENT_QUOTA_WINDOW_SECS)
}

/// Checks if a thread's rollout log indicates an active turn or an incomplete turn needing resumption.
#[allow(dead_code)]
pub fn is_thread_rollout_in_progress(codex_home: &std::path::Path, thread_id: &str) -> bool {
    matches!(
        inspect_thread_rollout_state(codex_home, thread_id),
        ThreadRolloutState::ActiveInProgress | ThreadRolloutState::InterruptedByQuota
    )
}

/// Detects active threads in progress by inspecting lock files in ~/.codex/thread-writer-locks/
/// and recent threads in state_5.sqlite, verifying their latest rollout events.
pub fn detect_in_progress_threads() -> Vec<String> {
    let codex_home = crate::storage::codex_home();
    let locks_dir = codex_home.join("thread-writer-locks");
    let mut in_progress = Vec::new();
    let now = chrono::Utc::now().timestamp();

    // 1. Check lock files held by running processes (codex app-server)
    if let Ok(entries) = std::fs::read_dir(&locks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if fname.starts_with('.') || !fname.ends_with(".lock") {
                continue;
            }

            let thread_id = &fname[..fname.len() - 5];
            if thread_id.is_empty() || !is_user_thread(&codex_home, thread_id) {
                continue;
            }

            // Open the lock file to test if another process holds a lock on it
            let file = match std::fs::OpenOptions::new()
                .read(true)
                .write(true)
                .open(&path)
            {
                Ok(f) => f,
                Err(_) => match std::fs::File::open(&path) {
                    Ok(f) => f,
                    Err(_) => continue,
                },
            };

            let is_locked = file.try_lock_exclusive().is_err();
            if !is_locked {
                let _ = file.unlock();
                continue;
            }

            // The lock is held by a running process (codex app-server).
            // Check rollout state: resume if mid-turn active, interrupted by quota, or turn was aborted/interrupted.
            // Never resume cleanly completed turns.
            match inspect_thread_rollout_state(&codex_home, thread_id) {
                ThreadRolloutState::ActiveInProgress => {
                    in_progress.push(thread_id.to_string());
                }
                ThreadRolloutState::InterruptedByQuota => {
                    if recent_quota_failure(&codex_home, thread_id, now, RECENT_QUOTA_WINDOW_SECS) {
                        in_progress.push(thread_id.to_string());
                    }
                }
                _ => {}
            }
        }
    }

    // 2. Also check top 30 recent threads from state_5.sqlite if they were interrupted
    // or failed due to quota/credit exhaustion within the quota window.
    for tid in detect_recent_quota_blocked_user_threads() {
        if !in_progress.iter().any(|existing| existing == &tid) {
            in_progress.push(tid);
        }
    }

    // Only a pre-restart manifest can distinguish our interruption from a
    // user's Stop. Treat it as a hint, not authority: stale manifests may
    // contain subagents, archived tasks, or rows removed by Desktop.
    append_eligible_pending(
        &codex_home,
        &mut in_progress,
        crate::recovery::load_pending().unwrap_or_default(),
    );
    if let Ok(ownerless) = crate::recovery::load_ownerless_pending() {
        in_progress.retain(|id| !ownerless.contains(id));
    } else {
        // A broken journal must not cause a new restart to revive an old
        // ownerless target under an unverified account.
        return Vec::new();
    }
    in_progress
}

pub(super) fn append_eligible_pending(
    codex_home: &std::path::Path,
    in_progress: &mut Vec<String>,
    pending: Vec<String>,
) {
    let now = chrono::Utc::now().timestamp();
    for tid in pending {
        if !is_user_thread(codex_home, &tid) || in_progress.contains(&tid) {
            continue;
        }
        match inspect_thread_rollout_state(codex_home, &tid) {
            ThreadRolloutState::InterruptedByQuota => {
                if recent_quota_failure(codex_home, &tid, now, RECENT_QUOTA_WINDOW_SECS) {
                    in_progress.push(tid);
                }
            }
            ThreadRolloutState::ActiveInProgress | ThreadRolloutState::TurnAborted => {
                let is_recent = get_thread_updated_at(codex_home, &tid)
                    .and_then(|updated| now.checked_sub(updated))
                    .is_some_and(|age| (0..=RECENT_QUOTA_WINDOW_SECS).contains(&age));
                if is_recent {
                    in_progress.push(tid);
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[path = "thread_detection_service.test.rs"]
mod tests;
