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
            // Final check on primary thread UI
            poll_and_trigger_ui_resume(6, Duration::from_millis(400));
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
    if wait_for_app_exit_with(
        is_codex_app_running,
        CODEX_EXIT_GRACE_PERIOD,
        CODEX_EXIT_POLL_INTERVAL,
    ) {
        return Ok(());
    }

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

    Ok(())
}

fn launch_codex_app() -> Result<(), String> {
    let status = Command::new("open")
        .arg("-a")
        .arg("/Applications/ChatGPT.app")
        .status()
        .map_err(|error| format!("Account switched, but Codex could not be relaunched: {error}"))?;
    if !status.success() {
        return Err("Account switched, but Codex could not be relaunched".to_string());
    }
    Ok(())
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
    let query = format!("SELECT id FROM threads WHERE archived = 0 ORDER BY updated_at DESC LIMIT {};", limit);
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

/// Detects active threads in progress by inspecting lock files in ~/.codex/thread-writer-locks/
/// and recent threads in state_5.sqlite, verifying their latest rollout events.
pub fn detect_in_progress_threads() -> Vec<String> {
    let codex_home = crate::storage::codex_home();
    let locks_dir = codex_home.join("thread-writer-locks");
    let mut in_progress = Vec::new();

    // 1. Check lock files held by running processes (codex app-server)
    if let Ok(entries) = std::fs::read_dir(&locks_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            if fname.starts_with('.') || !fname.ends_with(".lock") {
                continue;
            }

            let thread_id = &fname[..fname.len() - 5];
            if thread_id.is_empty() {
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
            // Check if rollout event history indicates an active or interrupted turn.
            if is_thread_rollout_in_progress(&codex_home, thread_id) {
                in_progress.push(thread_id.to_string());
            }
        }
    }

    // 2. Also check top 5 recent threads from state_5.sqlite for quota/credit exhaustion aborts
    let recent_threads = get_most_recent_threads(&codex_home, 5);
    for tid in recent_threads {
        if !in_progress.iter().any(|existing| existing == &tid) {
            if is_thread_rollout_in_progress(&codex_home, &tid) {
                in_progress.push(tid);
            }
        }
    }

    in_progress
}

/// Checks if a thread's rollout log indicates an active turn or an incomplete turn needing resumption.
pub fn is_thread_rollout_in_progress(codex_home: &std::path::Path, thread_id: &str) -> bool {
    // 1. Try querying sqlite3 ~/.codex/state_5.sqlite for rollout_path
    let state_sqlite = codex_home.join("state_5.sqlite");
    let mut rollout_path = None;

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
                    rollout_path = Some(std::path::PathBuf::from(p_str));
                }
            }
        }
    }

    // 2. Fallback: scan ~/.codex/sessions for files matching thread_id
    if rollout_path.is_none() {
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

        if let Some(first) = candidates.into_iter().next() {
            rollout_path = Some(first);
        }
    }

    // 3. Inspect the last few lines of the rollout file
    if let Some(path) = rollout_path {
        if let Ok(file) = std::fs::File::open(&path) {
            use std::io::{BufRead, BufReader};
            let reader = BufReader::new(file);
            let mut last_lines: Vec<String> = Vec::new();
            for line in reader.lines().flatten() {
                let trimmed = line.trim();
                if !trimmed.is_empty() {
                    if last_lines.len() >= 6 {
                        last_lines.remove(0);
                    }
                    last_lines.push(trimmed.to_string());
                }
            }

            if let Some(last_line) = last_lines.last() {
                if let Ok(val) = serde_json::from_str::<serde_json::Value>(last_line) {
                    let payload = val.get("payload");
                    let payload_type = payload
                        .and_then(|p| p.get("type"))
                        .and_then(|t| t.as_str());

                    if payload_type == Some("task_complete") {
                        // Check if the task failed with an error (e.g. usage/rate limit, workspace out of credits)
                        if let Some(error) = payload.and_then(|p| p.get("error")) {
                            if !error.is_null() {
                                return true;
                            }
                        }
                        // Also check if recent tail events indicated credit or rate limit exhaustion
                        for prev in &last_lines {
                            if prev.contains("workspace_owner_credits_depleted")
                                || prev.contains("usage_limit_exceeded")
                                || prev.contains("out of credits")
                                || prev.contains("rate_limit_reached_type")
                            {
                                return true;
                            }
                        }
                        // Clean completion with no error
                        return false;
                    }

                    // Any other payload type at the end of the rollout indicates an interrupted/in-progress turn
                    return true;
                }
            }
        }
    }

    false
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
}
