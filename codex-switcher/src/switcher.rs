use crate::models::AuthJson;
use crate::storage::{load_accounts, read_active_auth_json, save_accounts, write_active_auth_json};
use chrono::Utc;
use fs2::FileExt;
use std::process::Command;
use std::thread::sleep;
use std::time::{Duration, Instant};

const CODEX_EXIT_GRACE_PERIOD: Duration = Duration::from_secs(3);
const CODEX_EXIT_POLL_INTERVAL: Duration = Duration::from_millis(250);
const CODEX_APP_EXECUTABLE: &str = "/Applications/ChatGPT.app/Contents/MacOS/ChatGPT";

/// Resolves a user-provided account query to an account index.
/// Matching order:
/// 1. Exact canonical ID (<email>:<account_id>)
/// 2. Exact nickname (`name`)
/// 3. Exact ChatGPT workspace account_id UUID
/// 4. Unambiguous exact email
/// 5. Unambiguous prefix of canonical ID, UUID, or nickname (len >= 3)
pub fn resolve_target_account_idx(
    accounts: &[crate::models::AccountConfig],
    query: &str,
) -> Result<usize, String> {
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
        return Err(format!(
            "Multiple accounts share nickname '{}'. Please specify by full ID.",
            q
        ));
    }

    // 3. Exact account_id (workspace UUID) match
    let ws_matches: Vec<usize> = accounts
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.account_id.trim().eq_ignore_ascii_case(q)
                || a.tokens
                    .account_id
                    .as_deref()
                    .map(|t| t.trim().eq_ignore_ascii_case(q))
                    == Some(true)
        })
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
                    || a.name
                        .as_deref()
                        .map(|n| n.to_lowercase().starts_with(&q.to_lowercase()))
                        .unwrap_or(false)
            })
            .map(|(i, _)| i)
            .collect();
        if prefix_matches.len() == 1 {
            return Ok(prefix_matches[0]);
        }
    }

    Err(format!(
        "Account with ID, nickname, or email '{}' not found",
        query
    ))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SwitchOutcome {
    pub recovery_error: Option<String>,
}

fn prioritize_primary(targets: &mut Vec<String>, primary: Option<&String>) {
    let Some(primary) = primary else { return };
    if let Some(index) = targets.iter().position(|id| id == primary) {
        targets.remove(index);
    }
    targets.insert(0, primary.clone());
}

fn prioritize_primary_if_user(
    codex_home: &std::path::Path,
    targets: &mut Vec<String>,
    primary: Option<&String>,
) -> bool {
    if primary.is_some_and(|id| !is_user_thread(codex_home, id)) {
        return false;
    }
    prioritize_primary(targets, primary);
    true
}

fn relaunch_after_failed_transition(error: String) -> String {
    match launch_codex_app() {
        Ok(_) => format!("{error}; Codex was relaunched with the previous account state"),
        Err(relaunch_error) => {
            format!("{error}; emergency Codex relaunch also failed: {relaunch_error}")
        }
    }
}

fn keep_codex_available_after_failure(error: String) -> String {
    if is_codex_app_running() {
        return error;
    }
    match launch_codex_app() {
        Ok(pids) => {
            format!("{error}; Codex was relaunched after the failed automation with pids={pids:?}")
        }
        Err(relaunch_error) => {
            format!("{error}; emergency Codex relaunch also failed: {relaunch_error}")
        }
    }
}

pub fn switch_to_account(
    account_id: &str,
    restart_app: bool,
    notify: bool,
) -> Result<SwitchOutcome, String> {
    let _operation = crate::recovery::operation_lock()?;
    let mut accounts_file = load_accounts()?;
    let target_idx = resolve_target_account_idx(&accounts_file.accounts, account_id)?;

    let target_account = accounts_file.accounts[target_idx].clone();

    // Guard: reject switching to an account that requires re-login until relogin is completed
    if target_account.needs_relogin() {
        let relogin_hint = target_account.name.as_deref().unwrap_or(&target_account.id);
        return Err(format!(
            "Account '{}' ({}) requires re-login before switching. Please run 'cxi relogin \"{}\"' first.",
            target_account.display_name(),
            target_account.email,
            relogin_hint
        ));
    }

    // Redundant switch guard: if target account is already active, return Ok(()) immediately.
    let active_id = accounts_file.active_account_id.as_deref();
    let is_already_active = active_id
        .map(|id| id.eq_ignore_ascii_case(&target_account.id))
        .unwrap_or(false)
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
        return Ok(SwitchOutcome {
            recovery_error: None,
        });
    }

    let app_was_running = restart_app && is_codex_app_running();
    if app_was_running
        && std::env::var_os("CODEX_RESTART_WORKER").is_some()
        && crate::recovery::restart_cancellation_requested()
    {
        return Err("Restart cancelled before Codex shutdown".into());
    }

    // Detect in-progress threads before gracefully terminating the app
    let running_threads = if app_was_running {
        let mut threads = detect_in_progress_threads();
        if let Ok(primary) =
            std::env::var("CODEX_PRIMARY_THREAD").or_else(|_| std::env::var("CODEX_THREAD_ID"))
        {
            let primary = clean_thread_id(&primary);
            let _ = prioritize_primary_if_user(
                &crate::storage::codex_home(),
                &mut threads,
                Some(&primary),
            );
        }
        if !threads.is_empty() {
            println!(
                "📋 Detected {} active in-progress thread(s) before restart: {:?}",
                threads.len(),
                threads
            );
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

    let mut recovery_banner = if app_was_running {
        crate::recovery::arm_automation_cooldown()?;
        Some(crate::recovery::RecoveryBanner::start(
            running_threads.len(),
        )?)
    } else {
        None
    };
    if app_was_running && !running_threads.is_empty() {
        crate::recovery::preflight_desktop_dispatch()?;
    }

    // 2. Stop the desktop app before replacing credentials. A graceful exit is
    // the persistence boundary for active thread history and SQLite WAL state.
    // Never force-kill it: if it cannot flush and exit, leave auth untouched.
    if app_was_running {
        crate::recovery::save_pending(&running_threads)?;
        stop_codex_app_gracefully()?;
        // The first journal makes the target list durable before shutdown. The
        // second checkpoint is the verification boundary: it excludes work and
        // abort records flushed by the old Desktop from post-restart proof.
        if let Err(error) = crate::recovery::save_pending(&running_threads) {
            return Err(relaunch_after_failed_transition(error));
        }
    }

    // 3. Atomically write to ~/.codex/auth.json
    if let Err(error) = write_active_auth_json(&current_auth) {
        return Err(if app_was_running {
            relaunch_after_failed_transition(error)
        } else {
            error
        });
    }

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

    // 5. Relaunch the desktop first, then dispatch through its own queue/UI.
    // A separate `codex exec resume` process would own the thread writer lock
    // and make the desktop show "This is open in another app".
    let recovery_error = if app_was_running {
        match launch_codex_app() {
            Ok(launched_pids) => {
                let recovery_result = crate::recovery::recover_threads_with_banner(
                    &running_threads,
                    crate::recovery::RecoveryMode::CapturedRestart,
                    recovery_banner.as_ref().unwrap(),
                );
                drop(recovery_banner.take());
                let stability_result = crate::recovery::verify_desktop_stable(&launched_pids);
                match (recovery_result, stability_result) {
                    (Ok(()), Ok(())) => None,
                    (Err(recovery), Ok(())) => Some(recovery),
                    (Ok(()), Err(stability)) => Some(stability),
                    (Err(recovery), Err(stability)) => Some(format!(
                        "{recovery}; desktop stability also failed: {stability}"
                    )),
                }
                .map(keep_codex_available_after_failure)
            }
            Err(error) => {
                drop(recovery_banner.take());
                Some(keep_codex_available_after_failure(error))
            }
        }
    } else {
        None
    };
    if app_was_running {
        crate::recovery::arm_automation_cooldown()?;
    }
    // 6. Send macOS user notification
    if notify {
        send_macos_notification(
            &format!("Switched to {}", target_account.display_name()),
            &format!("5h Limit: {:.0}%", target_account.last_primary_percentage),
        );
    }

    Ok(SwitchOutcome { recovery_error })
}

fn parse_codex_app_pids(process_list: &str) -> Vec<u32> {
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

fn codex_app_pids() -> Vec<u32> {
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

fn stop_codex_app_gracefully() -> Result<(), String> {
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
            .args(["-n", "-a", "/Applications/ChatGPT.app"])
            .status()
            .map_err(|error| {
                format!("Account switched, but Codex could not be relaunched: {error}")
            })?;

        if !status.success() {
            if attempt == 3 {
                return Err("Account switched, but Codex could not be relaunched".to_string());
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

        println!(
            "⚠️ Codex app did not appear after attempt {}, retrying launch...",
            attempt
        );
        sleep(Duration::from_millis(500));
    }

    Err("Codex app launch was requested, but no stable main process appeared".to_string())
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

/// A quota failure is eligible for an automatic reset only briefly after it
/// happened. Older failed turns may have been intentionally abandoned and
/// must never spend a reset credit.
pub const RECENT_QUOTA_WINDOW_SECS: i64 = 4 * 3600;

/// Locates the JSONL rollout file for a thread, querying SQLite first and scanning sessions as fallback.
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

/// Finds only recent, unarchived, user-owned tasks whose latest terminal
/// rollout event is a quota failure. Unlike `detect_in_progress_threads`, it
/// intentionally excludes active, aborted, queued, and manifest-only tasks:
/// those are safe to recover but are not proof that a weekly reset is needed.
pub fn detect_recent_quota_blocked_user_threads() -> Vec<String> {
    let codex_home = crate::storage::codex_home();
    let now = chrono::Utc::now().timestamp();
    get_most_recent_threads(&codex_home, 30)
        .into_iter()
        .filter(|thread_id| is_user_thread(&codex_home, thread_id))
        .filter(|thread_id| {
            get_thread_updated_at(&codex_home, thread_id)
                .map(|updated| (now - updated).abs() <= RECENT_QUOTA_WINDOW_SECS)
                .unwrap_or(false)
        })
        .filter(|thread_id| {
            inspect_thread_rollout_state(&codex_home, thread_id)
                == ThreadRolloutState::InterruptedByQuota
        })
        .collect()
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
    in_progress
}

fn append_eligible_pending(
    codex_home: &std::path::Path,
    in_progress: &mut Vec<String>,
    pending: Vec<String>,
) {
    for tid in pending {
        if is_user_thread(codex_home, &tid) && !in_progress.contains(&tid) {
            in_progress.push(tid);
        }
    }
}

/// Recovery-only entry point. Uses the same verified pipeline as account switching.
pub fn resume_thread_interactive(thread_id: Option<&str>) -> Result<(), String> {
    let _operation = crate::recovery::operation_lock()?;
    if !is_codex_app_running() {
        return Err("Codex is not running; launch it before using resume".into());
    }
    let (targets, mode) = match thread_id {
        Some(tid) => (
            vec![clean_thread_id(tid)],
            crate::recovery::RecoveryMode::ExplicitTarget,
        ),
        None => (
            detect_in_progress_threads(),
            crate::recovery::RecoveryMode::DiscoveredOnly,
        ),
    };
    crate::recovery::recover_threads(&targets, mode)
}

/// Self-restart is supported: a detached worker, rather than the app's child
/// shell, owns the operation so recovery survives termination of this host.
pub fn dispatch_self_restart(args: &[String]) -> Result<bool, String> {
    if std::env::var_os("CODEX_RESTART_WORKER").is_some() {
        return Ok(false);
    }
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,comm="])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Cannot verify restart worker ancestry".into());
    }
    if !has_codex_ancestor(&String::from_utf8_lossy(&output.stdout), std::process::id())? {
        return Ok(false);
    }
    let home = crate::storage::codex_home();
    let label = "com.codex.switcher.restart-worker";
    let previous = Command::new("launchctl")
        .args(["list", label])
        .output()
        .map_err(|e| e.to_string())?;
    if previous.status.success() {
        if String::from_utf8_lossy(&previous.stdout).contains("\"PID\"") {
            return Err("A restart worker is already running".into());
        }
        let removed = Command::new("launchctl")
            .args(["remove", label])
            .status()
            .map_err(|e| e.to_string())?;
        if !removed.success() {
            return Err("Could not retire the completed restart worker".into());
        }
    }
    crate::recovery::arm_automation_cooldown()?;
    crate::recovery::clear_restart_cancellation()?;
    let directory = home.join("recovery-runs");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let operation_id = format!("{}-{}", Utc::now().timestamp_millis(), std::process::id());
    let log = directory.join(format!("restart-{operation_id}.log"));
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&log)
        .map_err(|e| e.to_string())?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut submit = Command::new("launchctl");
    submit
        .args(["submit", "-l", label, "-o"])
        .arg(&log)
        .arg("-e")
        .arg(&log)
        // A submitted job can be relaunched even after a short successful run.
        // Remove the one-shot label from inside the wrapper after the child
        // finishes. The operation claim remains the destructive at-most-once
        // guard if the wrapper itself is interrupted before that cleanup.
        .args([
            "--",
            "/bin/sh",
            "-c",
            "\"$@\"; /bin/launchctl remove com.codex.switcher.restart-worker >/dev/null 2>&1; exit 0",
            "codex-restart-once",
            "/usr/bin/env",
            "CODEX_RESTART_WORKER=1",
        ])
        .arg(format!("CODEX_RESTART_OPERATION={operation_id}"))
        .arg(format!("CODEX_HOME={}", home.display()));
    if let Ok(primary) = std::env::var("CODEX_THREAD_ID") {
        submit.arg(format!(
            "CODEX_PRIMARY_THREAD={}",
            clean_thread_id(&primary)
        ));
    }
    let status = submit
        .arg(executable)
        .arg("--restart-worker")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Could not launch independent restart worker".into());
    }
    println!(
        "RESTART_DISPATCHED job={label} log={} (scheduled, not yet verified)",
        log.display()
    );
    Ok(true)
}

fn has_codex_ancestor(processes: &str, mut pid: u32) -> Result<bool, String> {
    let rows: Vec<_> = processes
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim().splitn(3, char::is_whitespace);
            let id = fields.next()?.parse::<u32>().ok()?;
            let rest = line.trim().strip_prefix(&id.to_string())?.trim_start();
            let split = rest.find(char::is_whitespace)?;
            Some((id, rest[..split].parse::<u32>().ok()?, rest[split..].trim()))
        })
        .collect();
    for _ in 0..128 {
        if pid <= 1 {
            return Ok(false);
        }
        let (_, parent, executable) = rows
            .iter()
            .find(|row| row.0 == pid)
            .ok_or("Cannot resolve restart worker ancestry")?;
        if *executable == CODEX_APP_EXECUTABLE {
            return Ok(true);
        }
        pid = *parent;
    }
    Err("Cycle in restart worker ancestry".into())
}

pub fn restart_and_recover(
    delay_seconds: u64,
    primary_thread: Option<String>,
) -> Result<(), String> {
    let primary = primary_thread
        .or_else(|| std::env::var("CODEX_THREAD_ID").ok())
        .map(|id| clean_thread_id(&id));
    let mut args = vec![
        "restart".into(),
        "--delay-seconds".into(),
        delay_seconds.max(5).to_string(),
    ];
    if let Some(id) = &primary {
        args.extend(["--primary-thread".into(), id.clone()]);
    }
    if std::env::var_os("CODEX_RESTART_WORKER").is_none() && dispatch_self_restart(&args)? {
        return Ok(());
    }
    sleep(Duration::from_secs(delay_seconds));
    if std::env::var_os("CODEX_RESTART_WORKER").is_some()
        && crate::recovery::restart_cancellation_requested()
    {
        println!("WORKER_CANCELLED phase=pre_shutdown");
        return Ok(());
    }
    let _operation = crate::recovery::operation_lock()?;
    crate::recovery::arm_automation_cooldown()?;
    if !is_codex_app_running() {
        return Err("Codex is not running".into());
    }
    let mut targets = detect_in_progress_threads();
    if !prioritize_primary_if_user(
        &crate::storage::codex_home(),
        &mut targets,
        primary.as_ref(),
    ) {
        return Err("Primary task is absent, archived, or a subagent; refusing restart".into());
    }
    println!(
        "RESTART_BEGIN old_pids={:?} targets={:?}",
        codex_app_pids(),
        targets
    );
    let banner = crate::recovery::RecoveryBanner::start(targets.len())?;
    if !targets.is_empty() {
        crate::recovery::preflight_desktop_dispatch()?;
    }
    crate::recovery::save_pending(&targets)?;
    stop_codex_app_gracefully()?;
    // Re-checkpoint only after the old process has fully exited, so recovery
    // cannot be falsely verified by work flushed during shutdown.
    crate::recovery::save_pending(&targets)?;
    let launched_pids = match launch_codex_app() {
        Ok(pids) => pids,
        Err(error) => {
            drop(banner);
            return Err(keep_codex_available_after_failure(error));
        }
    };
    println!("RESTART_LAUNCHED new_pids={launched_pids:?}");
    let recovery_result = crate::recovery::recover_threads_with_banner(
        &targets,
        crate::recovery::RecoveryMode::CapturedRestart,
        &banner,
    );
    drop(banner);
    let stability_result = crate::recovery::verify_desktop_stable(&launched_pids);
    match (recovery_result, stability_result) {
        (Ok(()), Ok(())) => {}
        (Err(recovery), Ok(())) => return Err(keep_codex_available_after_failure(recovery)),
        (Ok(()), Err(stability)) => return Err(keep_codex_available_after_failure(stability)),
        (Err(recovery), Err(stability)) => {
            return Err(keep_codex_available_after_failure(format!(
                "{recovery}; desktop stability also failed: {stability}"
            )))
        }
    };
    for target in &targets {
        match inspect_thread_rollout_state(&crate::storage::codex_home(), target) {
            ThreadRolloutState::ActiveInProgress | ThreadRolloutState::CleanCompleted => {}
            state => {
                return Err(format!(
                    "Recovered task {target} ended in delayed state {state:?} during stabilization"
                ))
            }
        }
    }
    crate::recovery::arm_automation_cooldown()
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
    fn parses_only_the_exact_codex_app_executable() {
        let process_list = format!(
            "  42 {CODEX_APP_EXECUTABLE}\n\
             43 /Applications/ChatGPT.app/Contents/Frameworks/Codex Helper.app/Contents/MacOS/Codex Helper\n\
             44 /Applications/Other.app/Contents/MacOS/ChatGPT\n"
        );

        assert_eq!(parse_codex_app_pids(&process_list), vec![42]);
    }

    #[test]
    fn explicit_primary_is_always_first_and_never_duplicated() {
        let primary = "01a098c2-0fae-74d2-a80c-45d89e910e79".to_string();
        let mut targets = vec!["other".to_string(), primary.clone()];
        prioritize_primary(&mut targets, Some(&primary));
        assert_eq!(targets, [primary.clone(), "other".to_string()]);
        prioritize_primary(&mut targets, Some(&primary));
        assert_eq!(targets, [primary, "other".to_string()]);
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
                last_weekly_reset_time: None,
                last_weekly_reset_after_seconds: None,
                last_credits: None,
                last_error: None,
                last_checked: None,
                plan_multiplier: None,
                multiplier_is_manual: None,
                last_multiplier_checked: None,
                organization_name: None,
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
                last_weekly_reset_time: None,
                last_weekly_reset_after_seconds: None,
                last_credits: None,
                last_error: None,
                last_checked: None,
                plan_multiplier: None,
                multiplier_is_manual: None,
                last_multiplier_checked: None,
                organization_name: None,
            },
        ];

        // 1. Resolve by exact canonical ID
        assert_eq!(
            resolve_target_account_idx(&accounts, "user@example.com:3f533057-4bac-44ea"),
            Ok(0)
        );

        // 2. Resolve by nickname
        assert_eq!(resolve_target_account_idx(&accounts, "personal"), Ok(0));
        assert_eq!(resolve_target_account_idx(&accounts, "WORK"), Ok(1));

        // 3. Resolve by short UUID prefix
        assert_eq!(resolve_target_account_idx(&accounts, "3f5330"), Ok(0));
        assert_eq!(resolve_target_account_idx(&accounts, "26a1ef"), Ok(1));

        // 4. Resolve by email
        assert_eq!(
            resolve_target_account_idx(&accounts, "dev@company.com"),
            Ok(1)
        );

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
    fn user_thread_lookup_fails_closed_for_missing_archived_and_subagent_rows() {
        let root = std::env::temp_dir().join(format!(
            "codex-user-thread-test-{}-{}",
            std::process::id(),
            std::thread::current().name().unwrap_or("unnamed")
        ));
        std::fs::create_dir_all(&root).unwrap();
        let user_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48b";
        let subagent_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48c";
        let archived_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48d";
        let missing_id = "01a07d3c-3008-75c2-87a6-2c5c75f0e48e";

        assert!(!is_user_thread(&root, user_id));
        let database = root.join("state_5.sqlite");
        let sql = format!(
            "CREATE TABLE threads (id TEXT PRIMARY KEY, archived INTEGER, thread_source TEXT);\
             INSERT INTO threads VALUES ('{user_id}', 0, 'cli');\
             INSERT INTO threads VALUES ('{subagent_id}', 0, 'subagent');\
             INSERT INTO threads VALUES ('{archived_id}', 1, 'cli');"
        );
        let result = Command::new("/usr/bin/sqlite3")
            .arg(&database)
            .arg(sql)
            .status()
            .unwrap();
        assert!(result.success());
        assert!(is_user_thread(&root, user_id));
        assert!(!is_user_thread(&root, subagent_id));
        assert!(!is_user_thread(&root, archived_id));
        assert!(!is_user_thread(&root, missing_id));
        assert!(!is_user_thread(&root, "not-a-thread-id' OR 1=1 --"));

        let mut detected = vec![user_id.to_string()];
        append_eligible_pending(
            &root,
            &mut detected,
            vec![
                user_id.to_string(),
                subagent_id.to_string(),
                archived_id.to_string(),
                missing_id.to_string(),
            ],
        );
        assert_eq!(detected, vec![user_id.to_string()]);

        let mut primary_targets = Vec::new();
        assert!(!prioritize_primary_if_user(
            &root,
            &mut primary_targets,
            Some(&subagent_id.to_string())
        ));
        assert!(primary_targets.is_empty());
        assert!(prioritize_primary_if_user(
            &root,
            &mut primary_targets,
            Some(&user_id.to_string())
        ));
        assert_eq!(primary_targets, vec![user_id.to_string()]);

        std::fs::remove_dir_all(root).unwrap();
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
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"Investigate bug"}}"#
                .to_string(),
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#
                .to_string(),
            r#"{"type":"event_msg","payload":{"type":"item_completed","thread_id":"th-3"}}"#
                .to_string(),
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

    #[test]
    fn detects_when_self_restart_needs_an_independent_worker() {
        let rows = format!("1 0 /sbin/launchd\n100 1 {CODEX_APP_EXECUTABLE}\n200 100 /app-server\n300 200 /bin/zsh\n400 300 /cxi\n500 1 /cxi");
        assert!(has_codex_ancestor(&rows, 400).unwrap());
        assert!(!has_codex_ancestor(&rows, 500).unwrap());
        assert!(has_codex_ancestor(&rows, 999).is_err());
    }

    #[test]
    fn test_switch_to_account_rejects_relogin_needed() {
        let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("codex_relogin_guard_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_var("CODEX_HOME", &temp_dir);

        let acc = crate::models::AccountConfig {
            id: "user@example.com:uuid-1".to_string(),
            name: None,
            email: "user@example.com".to_string(),
            plan_type: "team".to_string(),
            account_id: "uuid-1".to_string(),
            tokens: crate::models::AuthTokens {
                access_token: "at_1".to_string(),
                refresh_token: Some("rt_1".to_string()),
                id_token: None,
                account_id: Some("uuid-1".to_string()),
            },
            enabled: true,
            priority: 1,
            last_primary_percentage: 100.0,
            last_reset_time: None,
            last_reset_after_seconds: None,
            last_weekly_percentage: None,
            last_weekly_reset_time: None,
            last_weekly_reset_after_seconds: None,
            last_credits: None,
            last_error: Some("401 Unauthorized (Session ended)".to_string()),
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
            organization_name: None,
        };

        let file = crate::models::AccountsFile {
            active_account_id: Some("other@example.com:uuid-2".to_string()),
            settings: Default::default(),
            accounts: vec![acc],
        };
        crate::storage::save_accounts(&file).unwrap();

        let err = switch_to_account("user@example.com:uuid-1", false, false).unwrap_err();
        assert!(err.contains("requires re-login"));
        assert!(err.contains("cxi relogin"));

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
