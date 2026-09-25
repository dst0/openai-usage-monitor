use super::*;
use fs2::FileExt;
use std::process::Command;

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
    get_most_recent_threads(&codex_home, 30)
        .into_iter()
        .filter(|thread_id| is_user_thread(&codex_home, thread_id))
        .filter(|thread_id| {
            get_thread_updated_at(&codex_home, thread_id)
                .map(|updated| (now - updated).abs() <= window_secs)
                .unwrap_or(false)
        })
        .filter(|thread_id| {
            inspect_thread_rollout_state(&codex_home, thread_id)
                == ThreadRolloutState::InterruptedByQuota
        })
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
                    let is_recent = get_thread_updated_at(&codex_home, thread_id)
                        .map(|updated| (now - updated).abs() <= RECENT_QUOTA_WINDOW_SECS)
                        .unwrap_or(true);
                    if is_recent {
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
        let is_recent = get_thread_updated_at(codex_home, &tid)
            .map(|updated| (now - updated).abs() <= RECENT_QUOTA_WINDOW_SECS)
            .unwrap_or(false);
        if !is_recent {
            continue;
        }
        match inspect_thread_rollout_state(codex_home, &tid) {
            ThreadRolloutState::ActiveInProgress
            | ThreadRolloutState::InterruptedByQuota
            | ThreadRolloutState::TurnAborted => {
                in_progress.push(tid);
            }
            _ => {}
        }
    }
}
