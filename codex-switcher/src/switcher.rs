use crate::models::AuthJson;
use crate::storage::{load_accounts, read_active_auth_json, save_accounts, write_active_auth_json};
use chrono::Utc;
use fs2::FileExt;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

const CODEX_EXIT_GRACE_PERIOD: Duration = Duration::from_secs(3);
const CODEX_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(250);

/// Resolves a user-provided account query to an account index.
/// Matching order:
/// 1. Exact canonical ID (<email>:<account_id>)
/// 2. Exact nickname (`name`)
/// 3. Exact ChatGPT workspace account_id UUID
/// 4. Unambiguous exact email
/// 5. Unambiguous prefix of canonical ID, UUID, or nickname (len >= 3)
pub fn resolve_target_account_idx(accounts: &[crate::models::AccountConfig], query: &str) -> Result<usize, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Account identifier cannot be empty".to_string());
    }

    // 1. Exact canonical ID match
    if let Some(pos) = accounts.iter().position(|a| a.id.eq_ignore_ascii_case(q)) {
        return Ok(pos);
    }

    // 2. Exact nickname match (case-insensitive)
    let nick_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| a.name.as_deref().map(|n| n.trim().eq_ignore_ascii_case(q)) == Some(true))
        .map(|(i, _)| i)
        .collect();
    if nick_matches.len() == 1 {
        return Ok(nick_matches[0]);
    } else if nick_matches.len() > 1 {
        return Err(format!("Multiple accounts share nickname '{}'. Please specify by full ID.", q));
    }

    // 3. Exact account_id (workspace UUID) match
    let ws_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| a.account_id.trim().eq_ignore_ascii_case(q) || a.tokens.account_id.as_deref().map(|t| t.trim().eq_ignore_ascii_case(q)) == Some(true))
        .map(|(i, _)| i)
        .collect();
    if ws_matches.len() == 1 {
        return Ok(ws_matches[0]);
    }

    // 4. Exact email match (unambiguous)
    let email_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| a.email.trim().eq_ignore_ascii_case(q))
        .map(|(i, _)| i)
        .collect();
    if email_matches.len() == 1 {
        return Ok(email_matches[0]);
    } else if email_matches.len() > 1 {
        let options: Vec<String> = email_matches
            .iter()
            .map(|&i| format!("{} ({})", accounts[i].display_name(), accounts[i].id))
            .collect();
        return Err(format!(
            "Multiple accounts found for email '{}': {}. Please specify by nickname or full ID.",
            q,
            options.join(", ")
        ));
    }

    // 5. Prefix match on ID, UUID, or nickname (if unambiguous and query length >= 3)
    if q.len() >= 3 {
        let prefix_matches: Vec<usize> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                a.id.to_lowercase().starts_with(&q.to_lowercase())
                    || a.account_id.to_lowercase().starts_with(&q.to_lowercase())
                    || a.name.as_deref().map(|n| n.to_lowercase().starts_with(&q.to_lowercase())).unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        if prefix_matches.len() == 1 {
            return Ok(prefix_matches[0]);
        }
    }

    Err(format!("Account with ID, nickname, or email '{}' not found", query))
}

pub fn switch_to_account(account_id: &str, restart_app: bool, notify: bool) -> Result<(), String> {
    let mut accounts_file = load_accounts()?;
    let target_idx = resolve_target_account_idx(&accounts_file.accounts, account_id)?;

    let target_account = accounts_file.accounts[target_idx].clone();

    // Redundant switch guard: if target account is already active, return Ok(()) immediately.
    let active_id = accounts_file.active_account_id.as_deref();
    let is_already_active = active_id.map(|id| id.eq_ignore_ascii_case(&target_account.id)).unwrap_or(false)
        || accounts_file
            .accounts
            .iter()
            .find(|a| Some(a.id.as_str()) == active_id)
            .map(|a| a.id.eq_ignore_ascii_case(&target_account.id))
            .unwrap_or(false)
        || (target_account.tokens.refresh_token.is_some()
            && accounts_file
                .accounts
                .iter()
                .find(|a| Some(a.id.as_str()) == active_id)
                .and_then(|a| a.tokens.refresh_token.as_ref())
                == target_account.tokens.refresh_token.as_ref());

    if is_already_active {
        return Ok(());
    }

    let app_was_running = restart_app && is_codex_app_running();

    // Detect in-progress threads before gracefully terminating the app
    let running_threads = if app_was_running {
        let threads = detect_in_progress_threads();
        if !threads.is_empty() {
            println!("📋 Detected {} active in-progress thread(s) before restart: {:?}", threads.len(), threads);
        }
        threads
    } else {
        Vec::new()
    };

    // 1. Prepare new auth.json
    let mut current_auth = read_active_auth_json().unwrap_or(AuthJson {
        auth_mode: Some("chatgpt".to_string()),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
    });
    let previous_auth = current_auth.clone();

    current_auth.tokens = Some(target_account.tokens.clone());
    current_auth.last_refresh = Some(Utc::now().to_rfc3339());

    // 2. Stop the desktop app before replacing credentials. A graceful exit is
    // the persistence boundary for active thread history and SQLite WAL state.
    // Never force-kill it: if it cannot flush and exit, leave auth untouched.
    if app_was_running {
        stop_codex_app_gracefully()?;
    }

    // 3. Atomically write to ~/.codex/auth.json
    write_active_auth_json(&current_auth)?;

    // 4. Update active_account_id in accounts.json. Restore the previous auth
    // if this second half of the local transaction fails.
    accounts_file.active_account_id = Some(target_account.id.clone());
    if let Err(error) = save_accounts(&accounts_file) {
        let restore_result = write_active_auth_json(&previous_auth);
        if app_was_running {
            let _ = launch_codex_app();
        }
        return match restore_result {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!(
                "Failed to update account state ({error}); restoring the previous authentication also failed ({restore_error})"
            )),
        };
    }

    // 5. Relaunch only when this switch actually stopped a running app.
    if app_was_running {
        launch_codex_app()?;

        let codex_home = dirs::home_dir().map(|h| h.join(".codex")).unwrap_or_default();
        // Determine primary thread to focus in ChatGPT UI
        let primary_thread = running_threads.first().cloned().or_else(|| {
            get_most_recent_threads(&codex_home, 1).into_iter().next()
        });

        // Wait for Codex App and its app-server to initialize, then resume threads
        if !running_threads.is_empty() {
            println!("⏳ Waiting for Codex App to initialize before resuming {} thread(s)...", running_threads.len());
            sleep(Duration::from_secs(3));
            resume_threads(&running_threads, "continue");
        } else {
            sleep(Duration::from_secs(2));
        }

        // Cycle through all running threads in the UI to trigger Accessibility Resume on each
        if running_threads.len() > 1 {
            println!("🔄 Cycling UI through {} threads to unpause any interrupted states...", running_threads.len());
            for tid in &running_threads {
                open_thread_in_codex(tid);
                sleep(Duration::from_millis(400));
                let _ = poll_and_trigger_ui_resume(3, Duration::from_millis(200));
            }
        }

        // Navigate ChatGPT UI directly to the primary thread so it is visible to the user
        if let Some(ref tid) = primary_thread {
            open_thread_in_codex(tid);
            sleep(Duration::from_millis(500));
            if running_threads.iter().any(|r| r == tid) {
                // Final check on primary thread UI only if it was actually in running_threads
                poll_and_trigger_ui_resume(6, Duration::from_millis(400));
            }
        }
    }

    // 6. Send macOS user notification
    if notify {
        send_macos_notification(
            &format!("Switched to {}", target_account.display_name()),
            &format!("5h Limit: {:.0}%", target_account.last_primary_percentage),
        );
    }

    Ok(())
}

pub fn is_codex_app_running() -> bool {
    let output = Command::new("pgrep")
        .arg("-f")
        .arg("/Applications/ChatGPT.app/Contents/MacOS/ChatGPT")
        .output();

    matches!(output, Ok(o) if o.status.success() && !o.stdout.is_empty())
}

fn stop_codex_app_gracefully() -> Result<(), String> {
    // 1. Send SIGTERM to ChatGPT main process.
    // Chromium catches SIGTERM to flush SQLite databases, cookies, and WAL logs cleanly,
    // while completely bypassing the interactive GUI beforeunload ("Leave site?") prompt.
    let _ = Command::new("pkill")
        .arg("-TERM")
        .arg("-f")
        .arg("/Applications/ChatGPT.app/Contents/MacOS/ChatGPT")
        .output();

    // 2. Wait up to 3 seconds for graceful process exit
    let exited = wait_for_app_exit_with(
        is_codex_app_running,
        CODEX_EXIT_GRACE_PERIOD,
        CODEX_EXIT_POLL_INTERVAL,
    );

    if !exited {
        // 3. Fallback: if not exited within 3s, terminate so account switch does not stall
        let _ = Command::new("pkill")
            .arg("-KILL")
            .arg("-f")
            .arg("/Applications/ChatGPT.app/Contents/MacOS/ChatGPT")
            .output();

        let _ = wait_for_app_exit_with(
            is_codex_app_running,
            Duration::from_secs(2),
            CODEX_EXIT_POLL_INTERVAL,
        );
    }

    // Cooldown: allow LaunchServices, loginwindow, and Chromium auxiliary helpers
    // to cleanly deregister the old app ASN before any relaunch attempt.
    sleep(Duration::from_millis(600));

    Ok(())
}

fn launch_codex_app() -> Result<(), String> {
    // Attempt launching with verification that the application process actually appears.
    // LaunchServices can sometimes ignore an open request if it was issued while
    // the previous process teardown was still registering, so retry with backoff.
    for attempt in 1..=3 {
        let status = Command::new("open")
            .arg("-a")
            .arg("/Applications/ChatGPT.app")
            .status()
            .map_err(|error| format!("Account switched, but Codex could not be relaunched: {error}"))?;

        if !status.success() {
            if attempt == 3 {
                return Err("Account switched, but Codex could not be relaunched".to_string());
            }
            sleep(Duration::from_millis(500));
            continue;
        }

        // Wait up to 3 seconds to verify the process actually appeared
        let deadline = Instant::now() + Duration::from_millis(3000);
        while Instant::now() < deadline {
            if is_codex_app_running() {
                return Ok(());
            }
            sleep(Duration::from_millis(200));
        }

        println!("⚠️ Codex app did not appear after attempt {}, retrying launch...", attempt);
        sleep(Duration::from_millis(500));
    }

    if is_codex_app_running() {
        Ok(())
    } else {
        Err("Codex app launch was requested, but process did not start".to_string())
    }
}

fn wait_for_app_exit_with<F>(mut is_running: F, timeout: Duration, poll_interval: Duration) -> bool
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
    let script = format!(
        "display notification \"{}\" with title \"Codex Switcher\" subtitle \"{}\"",
        escape_applescript(message),
        escape_applescript(title)
    );
    let _ = Command::new("osascript").arg("-e").arg(script).output();
}

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

/// Navigates ChatGPT desktop application directly to a specific thread URL.
pub fn open_thread_in_codex(thread_id: &str) {
    let clean = clean_thread_id(thread_id);
    if clean.is_empty() {
        return;
    }
    println!("🧭 Opening thread '{}' in ChatGPT...", clean);
    let _ = Command::new("open")
        .arg(format!("codex://threads/{}", clean))
        .status();
}

/// Retrieves the most recently updated unarchived threads from state_5.sqlite.
pub fn get_most_recent_threads(codex_home: &std::path::Path, limit: usize) -> Vec<String> {
    let state_sqlite = codex_home.join("state_5.sqlite");
    if !state_sqlite.exists() {
        return Vec::new();
    }
    let query = format!(
        "SELECT id FROM threads WHERE archived = 0 AND (thread_source = 'user' OR thread_source IS NULL OR thread_source = '') ORDER BY updated_at DESC LIMIT {};",
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
    let state_sqlite = codex_home.join("state_5.sqlite");
    if !state_sqlite.exists() {
        return true;
    }
    let query = format!(
        "SELECT thread_source FROM threads WHERE id = '{}' LIMIT 1;",
        thread_id
    );
    if let Ok(output) = Command::new("/usr/bin/sqlite3")
        .arg(state_sqlite.to_str().unwrap_or(""))
        .arg(&query)
        .output()
    {
        if output.status.success() {
            let src = String::from_utf8_lossy(&output.stdout).trim().to_string();
            if src == "subagent" {
                return false;
            }
        }
    }
    true
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadRolloutState {
    /// Turn completed cleanly with task_complete and no error. Never resume.
    CleanCompleted,
    /// Turn ended with an error indicating usage/rate limits or credit exhaustion.
    InterruptedByQuota,
    /// Turn was aborted/interrupted by user or cancelled.
    TurnAborted,
    /// Turn was actively executing mid-flight (user message, tool call, reasoning in flight).
    ActiveInProgress,
    /// No significant events or unparseable.
    Unknown,
}

/// Locates the JSONL rollout file for a thread, querying SQLite first and scanning sessions as fallback.
pub fn find_thread_rollout_path(codex_home: &std::path::Path, thread_id: &str) -> Option<std::path::PathBuf> {
    let state_sqlite = codex_home.join("state_5.sqlite");
    if state_sqlite.exists() {
        let query = format!("SELECT rollout_path FROM threads WHERE id = '{}' LIMIT 1;", thread_id);
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

    fn scan_sessions(dir: &std::path::Path, thread_id: &str, matches: &mut Vec<std::path::PathBuf>) {
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
        let m_a = a.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let m_b = b.metadata().and_then(|m| m.modified()).unwrap_or(std::time::SystemTime::UNIX_EPOCH);
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
    let seek_pos = if len > max_bytes { len - max_bytes } else { 0 };
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
            Some("user_message")
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
pub fn inspect_thread_rollout_state(codex_home: &std::path::Path, thread_id: &str) -> ThreadRolloutState {
    if let Some(path) = find_thread_rollout_path(codex_home, thread_id) {
        let lines = read_rollout_tail_lines(&path, 131072);
        return inspect_thread_rollout_state_from_lines(&lines);
    }
    ThreadRolloutState::Unknown
}

/// Returns the unix timestamp of the thread's updated_at field from state_5.sqlite.
pub fn get_thread_updated_at(codex_home: &std::path::Path, thread_id: &str) -> Option<i64> {
    let state_sqlite = codex_home.join("state_5.sqlite");
    if !state_sqlite.exists() {
        return None;
    }
    let query = format!("SELECT updated_at FROM threads WHERE id = '{}' LIMIT 1;", thread_id);
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
    const RECENT_QUOTA_WINDOW_SECS: i64 = 4 * 3600; // 4 hours

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
            let file = match std::fs::OpenOptions::new().read(true).write(true).open(&path) {
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
            // Check rollout state: ONLY resume if mid-turn active or recently quota-exhausted.
            // Never resume cleanly completed or user-aborted threads!
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

    // 2. Also check top 30 recent threads from state_5.sqlite if they failed
    // due to quota/credit exhaustion within the quota window.
    let recent_threads = get_most_recent_threads(&codex_home, 30);
    for tid in recent_threads {
        if !in_progress.iter().any(|existing| existing == &tid) {
            let is_recent = get_thread_updated_at(&codex_home, &tid)
                .map(|updated| (now - updated).abs() <= RECENT_QUOTA_WINDOW_SECS)
                .unwrap_or(false);
            if is_recent {
                if inspect_thread_rollout_state(&codex_home, &tid) == ThreadRolloutState::InterruptedByQuota {
                    in_progress.push(tid);
                }
            }
        }
    }

    in_progress
}

/// Resumes threads by queueing a message (e.g. "continue") via codex queue CLI.
pub fn resume_threads(thread_ids: &[String], message: &str) {
    if thread_ids.is_empty() {
        return;
    }

    let app_codex = std::path::Path::new("/Applications/ChatGPT.app/Contents/Resources/codex");
    let codex_bin = if app_codex.exists() {
        app_codex.to_str().unwrap()
    } else {
        "codex"
    };

    for tid in thread_ids {
        println!("🚀 Auto-resuming thread '{}' with '{}'...", tid, message);
        let status = Command::new(codex_bin)
            .arg("queue")
            .arg("--thread")
            .arg(tid)
            .arg("--message")
            .arg(message)
            .status();

        match status {
            Ok(s) if s.success() => {
                println!("✅ Queued resumption message for thread '{}'", tid);
            }
            Ok(s) => {
                eprintln!("⚠️ Codex queue exited with code {:?} for thread '{}'", s.code(), tid);
            }
            Err(e) => {
                eprintln!("⚠️ Failed to execute codex queue for thread '{}': {}", tid, e);
            }
        }
    }
}

/// Interactively resumes a specific thread or the most recent active/interrupted thread.
/// Navigates ChatGPT UI directly to the thread, queues a continuation message,
/// and unpauses any paused UI elements.
pub fn resume_thread_interactive(thread_id: Option<&str>) -> Result<(), String> {
    let codex_home = crate::storage::codex_home();
    let target_tids = match thread_id {
        Some(tid) if !tid.trim().is_empty() => vec![clean_thread_id(tid)],
        _ => {
            let active = detect_in_progress_threads();
            if !active.is_empty() {
                active
            } else if let Some(recent) = get_most_recent_threads(&codex_home, 1).into_iter().next() {
                vec![recent]
            } else {
                return Err("No active or recent thread found to resume.".to_string());
            }
        }
    };

    println!("🚀 Resuming {} thread(s): {:?}", target_tids.len(), target_tids);
    resume_threads(&target_tids, "continue");

    // Cycle through all target threads to ensure UI unpauses on each
    for tid in &target_tids {
        open_thread_in_codex(tid);
        sleep(Duration::from_millis(400));
        let _ = poll_and_trigger_ui_resume(3, Duration::from_millis(200));
    }

    if let Some(primary) = target_tids.first() {
        open_thread_in_codex(primary);
        poll_and_trigger_ui_resume(5, Duration::from_millis(400));
    }
    Ok(())
}

/// Triggers the native macOS Accessibility "Resume" action on ChatGPT.app
/// to unpause any interrupted steer or paused queue.
pub fn trigger_codex_ui_resume() -> bool {
    // 1. Check if compiled helper binary exists
    let helper_names = [
        dirs::home_dir().map(|h| h.join(".local/bin/codex-ui-resume")),
        std::env::current_exe().ok().and_then(|p| p.parent().map(|d| d.join("codex-ui-resume"))),
    ];

    for candidate in helper_names.into_iter().flatten() {
        if candidate.exists() {
            if let Ok(status) = Command::new(&candidate).status() {
                if status.success() {
                    return true;
                }
            }
        }
    }

    // 2. Fallback to inline swift -e script
    const SWIFT_SCRIPT: &str = r#"
import Cocoa
import ApplicationServices

guard let app = NSRunningApplication.runningApplications(withBundleIdentifier: "com.openai.codex").first else {
    exit(1)
}
let axApp = AXUIElementCreateApplication(app.processIdentifier)
AXUIElementSetAttributeValue(axApp, "AXManualAccessibility" as CFString, true as CFTypeRef)

var windows: AnyObject?
guard AXUIElementCopyAttributeValue(axApp, kAXWindowsAttribute as CFString, &windows) == .success,
      let winList = windows as? [AXUIElement] else { exit(1) }

var resumed = false
func searchAndPress(el: AXUIElement, depth: Int = 0) {
    if depth > 25 { return }
    var role: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXRoleAttribute as CFString, &role)
    var desc: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXDescriptionAttribute as CFString, &desc)
    var title: AnyObject?
    AXUIElementCopyAttributeValue(el, kAXTitleAttribute as CFString, &title)
    
    let r = (role as? String) ?? ""
    let d = (desc as? String) ?? ""
    let t = (title as? String) ?? ""
    
    if r == "AXButton" && (d == "Resume" || t == "Resume") {
        let err = AXUIElementPerformAction(el, kAXPressAction as CFString)
        if err == .success { resumed = true }
    }
    
    var children: AnyObject?
    if AXUIElementCopyAttributeValue(el, kAXChildrenAttribute as CFString, &children) == .success,
       let childList = children as? [AXUIElement] {
        for c in childList { searchAndPress(el: c, depth: depth + 1) }
    }
}

for win in winList { searchAndPress(el: win) }
exit(resumed ? 0 : 1)
"#;

    if let Ok(status) = Command::new("swift").arg("-e").arg(SWIFT_SCRIPT).status() {
        return status.success();
    }

    false
}

/// Polls for the ChatGPT UI to finish hydrating and triggers Resume if needed.
pub fn poll_and_trigger_ui_resume(retries: usize, interval: Duration) -> bool {
    for i in 1..=retries {
        if trigger_codex_ui_resume() {
            println!("✅ Triggered UI Resume for active Codex thread (attempt {i}/{retries})");
            return true;
        }
        sleep(interval);
    }
    false
}


#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn returns_when_the_app_has_already_exited() {
        assert!(wait_for_app_exit_with(
            || false,
            Duration::ZERO,
            Duration::ZERO,
        ));
    }

    #[test]
    fn waits_until_the_app_exits() {
        let mut checks = 0;
        assert!(wait_for_app_exit_with(
            || {
                checks += 1;
                checks < 3
            },
            Duration::from_secs(1),
            Duration::ZERO,
        ));
        assert_eq!(checks, 3);
    }

    #[test]
    fn times_out_without_forcing_termination() {
        assert!(!wait_for_app_exit_with(
            || true,
            Duration::ZERO,
            Duration::ZERO,
        ));
    }

    #[test]
    fn test_resolve_target_account_idx() {
        use crate::models::{AccountConfig, AuthTokens};

        let accounts = vec![
            AccountConfig {
                id: "user@example.com:3f533057-4bac-44ea".to_string(),
                name: Some("personal".to_string()),
                email: "user@example.com".to_string(),
                plan_type: "pro".to_string(),
                account_id: "3f533057-4bac-44ea".to_string(),
                tokens: AuthTokens {
                    access_token: "tok1".to_string(),
                    refresh_token: None,
                    id_token: None,
                    account_id: Some("3f533057-4bac-44ea".to_string()),
                },
                enabled: true,
                priority: 1,
                last_primary_percentage: 100.0,
                last_reset_time: None,
                last_reset_after_seconds: None,
                last_weekly_percentage: None,
                last_credits: None,
                last_error: None,
                last_checked: None,
                plan_multiplier: None,
                multiplier_is_manual: None,
                last_multiplier_checked: None,
            },
            AccountConfig {
                id: "dev@company.com:26a1ef5c-ad94-460e".to_string(),
                name: Some("work".to_string()),
                email: "dev@company.com".to_string(),
                plan_type: "team".to_string(),
                account_id: "26a1ef5c-ad94-460e".to_string(),
                tokens: AuthTokens {
                    access_token: "tok2".to_string(),
                    refresh_token: None,
                    id_token: None,
                    account_id: Some("26a1ef5c-ad94-460e".to_string()),
                },
                enabled: true,
                priority: 2,
                last_primary_percentage: 100.0,
                last_reset_time: None,
                last_reset_after_seconds: None,
                last_weekly_percentage: None,
                last_credits: None,
                last_error: None,
                last_checked: None,
                plan_multiplier: None,
                multiplier_is_manual: None,
                last_multiplier_checked: None,
            },
        ];

        // 1. Resolve by exact canonical ID
        assert_eq!(resolve_target_account_idx(&accounts, "user@example.com:3f533057-4bac-44ea"), Ok(0));

        // 2. Resolve by nickname
        assert_eq!(resolve_target_account_idx(&accounts, "personal"), Ok(0));
        assert_eq!(resolve_target_account_idx(&accounts, "WORK"), Ok(1));

        // 3. Resolve by short UUID prefix
        assert_eq!(resolve_target_account_idx(&accounts, "3f5330"), Ok(0));
        assert_eq!(resolve_target_account_idx(&accounts, "26a1ef"), Ok(1));

        // 4. Resolve by email
        assert_eq!(resolve_target_account_idx(&accounts, "dev@company.com"), Ok(1));

        // 5. Unknown account
        assert!(resolve_target_account_idx(&accounts, "unknown").is_err());
    }

    #[test]
    fn test_clean_thread_id() {
        assert_eq!(
            clean_thread_id("01a07d3c-3008-75c2-87a6-2c5c75f0e48b"),
            "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
        );
        assert_eq!(
            clean_thread_id("codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b"),
            "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
        );
        assert_eq!(
            clean_thread_id("codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b/"),
            "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
        );
        assert_eq!(
            clean_thread_id("chatgpt://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b"),
            "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
        );
        assert_eq!(
            clean_thread_id("  codex://threads/01a07d3c-3008-75c2-87a6-2c5c75f0e48b  "),
            "01a07d3c-3008-75c2-87a6-2c5c75f0e48b"
        );
    }

    #[test]
    fn test_rollout_clean_completed_with_trailing_events_and_null_rate_limits() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Audit project"}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":5.0},"rate_limit_reached_type":null}}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-1","last_agent_message":"Work is finished!","error":null}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-1"}}"#.to_string(),
            r#"{"type":"token_usage_record","payload":{"tokens":123}}"#.to_string(),
        ];

        let state = inspect_thread_rollout_state_from_lines(&lines);
        assert_eq!(state, ThreadRolloutState::CleanCompleted);
    }

    #[test]
    fn test_rollout_interrupted_by_quota_exhaustion() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Run tests"}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-2","error":{"message":"You have hit your limit","codex_error_info":"usage_limit_exceeded"}}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-2"}}"#.to_string(),
        ];

        let state = inspect_thread_rollout_state_from_lines(&lines);
        assert_eq!(state, ThreadRolloutState::InterruptedByQuota);
    }

    #[test]
    fn test_rollout_turn_aborted_by_user() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Investigate bug"}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#.to_string(),
            r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-3"}}"#.to_string(),
        ];

        let state = inspect_thread_rollout_state_from_lines(&lines);
        assert_eq!(state, ThreadRolloutState::TurnAborted);
    }

    #[test]
    fn test_rollout_active_mid_turn() {
        let lines = vec![
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Build feature"}}"#.to_string(),
            r#"{"type":"response_item","payload":{"type":"custom_tool_call","name":"exec","input":"cargo build"}}"#.to_string(),
        ];

        let state = inspect_thread_rollout_state_from_lines(&lines);
        assert_eq!(state, ThreadRolloutState::ActiveInProgress);
    }

    #[test]
    fn test_detect_in_progress_live() {
        let in_progress = detect_in_progress_threads();
        println!("Live detected in-progress threads: {:?}", in_progress);
    }
}
