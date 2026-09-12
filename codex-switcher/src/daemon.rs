use crate::models::{AccountStatusEntry, AccountsFile, StatusFile};
use crate::quota::update_account_quota_cache;
use crate::storage::{
    daemon_lock_path, load_accounts, read_active_auth_json, save_accounts, write_active_auth_json, write_status_file,
};
use crate::strategy::{needs_switch, select_best_switch};
use crate::switcher::switch_to_account;
use chrono::Utc;
use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub fn should_notify_switch(
    last_notified_id: Option<&str>,
    current_active_id: Option<&str>,
    target_id: &str,
    notify_on_switch: bool,
    last_notified_time: Option<Instant>,
    now: Instant,
    cooldown: Duration,
) -> bool {
    if !notify_on_switch {
        return false;
    }
    if current_active_id == Some(target_id) {
        return false;
    }
    if last_notified_id == Some(target_id) {
        if let Some(last_time) = last_notified_time {
            if now.duration_since(last_time) < cooldown {
                return false;
            }
        }
    }
    true
}

pub fn should_skip_redundant_switch(
    last_switched_id: Option<&str>,
    target_id: &str,
    active_account_id: Option<&str>,
    last_switched_time: Option<Instant>,
    now: Instant,
    cooldown: Duration,
) -> bool {
    if active_account_id == Some(target_id) {
        return true;
    }
    if last_switched_id == Some(target_id) && active_account_id.is_none() {
        if let Some(last_time) = last_switched_time {
            if now.duration_since(last_time) < cooldown {
                return true;
            }
        }
    }
    false
}

/// Pure logic for synchronizing tokens from an AuthJson instance into AccountsFile.
pub fn sync_active_tokens_from_auth_obj(
    accounts_file: &mut AccountsFile,
    active_auth: &crate::models::AuthJson,
) -> bool {
    let Some(auth_tokens) = active_auth.tokens.as_ref() else {
        return false;
    };

    // Ignore empty or whitespace-only tokens (e.g. logged out session or dummy tokens)
    if auth_tokens.access_token.trim().is_empty() {
        return false;
    }

    let (auth_email, auth_plan) = crate::oauth::extract_jwt_metadata_from_tokens(auth_tokens);
    let auth_account_id = auth_tokens.account_id.as_deref();

    // Multi-vector matching: match only by tokens, account_id (when non-conflicting), or email.
    let matched_idx = crate::setup::find_existing_account_idx(
        &accounts_file.accounts,
        "",
        auth_email.as_deref().unwrap_or(""),
        auth_account_id.unwrap_or(""),
        auth_plan.as_deref(),
        Some(auth_tokens),
    );

    if let Some(idx) = matched_idx {
        let acc = &mut accounts_file.accounts[idx];
        let mut changed = false;

        if accounts_file.active_account_id.as_deref() != Some(&acc.id) {
            accounts_file.active_account_id = Some(acc.id.clone());
            changed = true;
        }

        if acc.tokens != *auth_tokens {
            acc.tokens = auth_tokens.clone();
            acc.last_error = None;
            changed = true;
        }
        if let Some(email) = auth_email {
            if acc.email != email {
                acc.email = email;
                changed = true;
            }
        }
        if let Some(plan) = auth_plan {
            if acc.plan_type != plan {
                acc.plan_type = plan;
                changed = true;
            }
        }
        if let Some(acc_id) = auth_account_id {
            if acc.account_id != acc_id {
                acc.account_id = acc_id.to_string();
                changed = true;
            }
        }

        changed
    } else {
        // Active session in auth.json is for an account NOT yet in accounts.json!
        // Only auto-register if it represents a genuinely authenticated account with an identifiable email.
        let valid_email = auth_email.as_ref().map(|e| {
            let t = e.trim();
            !t.is_empty() && t.contains('@') && !t.eq_ignore_ascii_case("user@openai.com") && !t.eq_ignore_ascii_case("current-user")
        }).unwrap_or(false);

        if !valid_email {
            return false;
        }

        // Auto-register it as a new account without destroying or mutating existing accounts.
        let added_id = crate::setup::add_account_to_accounts_file(
            accounts_file,
            "",
            auth_tokens.clone(),
            false,
        );
        let added_email = accounts_file
            .accounts
            .iter()
            .find(|a| a.id == added_id)
            .map(|a| a.email.clone())
            .unwrap_or_else(|| auth_email.unwrap_or_else(|| added_id.clone()));

        println!("✨ Auto-saved newly logged-in account '{}' ({}) from Codex app", added_id, added_email);
        crate::switcher::send_macos_notification(
            &format!("New Account Added: {}", added_id),
            &format!("Auto-saved {} from Codex app", added_email),
        );
        true
    }
}

/// Synchronizes active tokens from ~/.codex/auth.json into ~/.codex/accounts.json.
/// This prevents stale token errors when ChatGPT.app or Codex CLI refreshes credentials.
pub fn sync_active_tokens(accounts_file: &mut AccountsFile) -> Result<bool, String> {
    let active_auth = match read_active_auth_json() {
        Ok(a) => a,
        Err(_) => return Ok(false),
    };

    let changed = sync_active_tokens_from_auth_obj(accounts_file, &active_auth);
    if changed {
        save_accounts(accounts_file)?;
    }

    Ok(changed)
}

pub fn run_daemon_tick_with_state(
    last_switched_id: &mut Option<String>,
    last_switched_time: &mut Option<Instant>,
    last_notified_id: &mut Option<String>,
    last_notified_time: &mut Option<Instant>,
    auto_switch: bool,
) -> Result<(), String> {
    let mut accounts_file = load_accounts().unwrap_or_default();

    // 1. Auto-sync tokens from auth.json (auto-save new logins and sync refreshed credentials)
    let _ = sync_active_tokens(&mut accounts_file);

    if accounts_file.accounts.is_empty() {
        return Err("No accounts configured to monitor".to_string());
    }

    let current_active_id = accounts_file
        .active_account_id
        .clone()
        .unwrap_or_else(|| accounts_file.accounts[0].id.clone());
    accounts_file.active_account_id = Some(current_active_id.clone());

    // 2. Fetch fresh usage for all accounts
    let mut status_entries = Vec::new();
    let mut active_entry_idx = None;

    for (idx, acc) in accounts_file.accounts.iter_mut().enumerate() {
        let is_active = acc.id == current_active_id;
        if is_active {
            active_entry_idx = Some(idx);
        }

        if acc.enabled {
            let prev_tokens = acc.tokens.clone();
            update_account_quota_cache(acc);

            // If query failed with 401 on the active account, re-read auth.json and retry once
            if is_active && acc.last_error.as_deref().map(|e| e.contains("401")).unwrap_or(false) {
                if let Ok(fresh_auth) = read_active_auth_json() {
                    if let Some(fresh_tokens) = fresh_auth.tokens {
                        if fresh_tokens != acc.tokens {
                            acc.tokens = fresh_tokens;
                            update_account_quota_cache(acc);
                        }
                    }
                }
            }

            // If active account's tokens were refreshed during update_account_quota_cache,
            // write them back to auth.json atomically!
            if is_active && acc.tokens != prev_tokens {
                if let Ok(mut curr_auth) = read_active_auth_json() {
                    curr_auth.tokens = Some(acc.tokens.clone());
                    curr_auth.last_refresh = Some(Utc::now().to_rfc3339());
                    let _ = write_active_auth_json(&curr_auth);
                }
            }
        }

        status_entries.push(AccountStatusEntry {
            id: acc.id.clone(),
            name: acc.name.clone(),
            email: acc.email.clone(),
            plan_type: acc.plan_type.clone(),
            is_active,
            five_hour_percentage: acc.last_primary_percentage,
            weekly_percentage: acc.last_weekly_percentage,
            reset_time: acc.last_reset_time.clone(),
            reset_after_seconds: acc.last_reset_after_seconds,
            credits: acc.last_credits.unwrap_or(0),
            error: acc.last_error.clone(),
            plan_multiplier: acc.effective_multiplier(),
        });
    }

    // Save updated quota caches back to accounts.json
    // Reload fresh accounts from disk under lock to preserve any settings or accounts modified during network fetch!
    let mut fresh = load_accounts().unwrap_or_default();
    for updated in &accounts_file.accounts {
        if let Some(acc) = fresh.accounts.iter_mut().find(|a| a.id == updated.id) {
            acc.last_primary_percentage = updated.last_primary_percentage;
            acc.last_reset_time = updated.last_reset_time.clone();
            acc.last_reset_after_seconds = updated.last_reset_after_seconds;
            acc.last_weekly_percentage = updated.last_weekly_percentage;
            acc.last_credits = updated.last_credits;
            acc.last_error = updated.last_error.clone();
            acc.last_checked = updated.last_checked.clone();
            acc.tokens = updated.tokens.clone();
            if updated.multiplier_is_manual != Some(true) {
                acc.plan_multiplier = updated.plan_multiplier;
                acc.last_multiplier_checked = updated.last_multiplier_checked.clone();
            }
        }
    }
    let _ = save_accounts(&fresh);
    accounts_file.settings = fresh.settings;

    // 3. Write usage-status.json for Swift Menu Bar app
    let active_acc = active_entry_idx.map(|i| &accounts_file.accounts[i]);
    let status = StatusFile {
        timestamp: Utc::now().to_rfc3339(),
        active_account_id: Some(current_active_id.clone()),
        active_email: active_acc.map(|a| a.email.clone()),
        active_plan: active_acc.map(|a| a.plan_type.clone()),
        five_hour_percentage: active_acc.map(|a| a.last_primary_percentage).unwrap_or(100.0),
        weekly_percentage: active_acc.and_then(|a| a.last_weekly_percentage),
        reset_time: active_acc.and_then(|a| a.last_reset_time.clone()),
        reset_after_seconds: active_acc.and_then(|a| a.last_reset_after_seconds),
        credits: active_acc.and_then(|a| a.last_credits).unwrap_or(0),
        auto_switch_enabled: accounts_file.settings.auto_switch_enabled,
        auto_switch_business_only: accounts_file.settings.auto_switch_business_only,
        auto_switch_business_priority: accounts_file.settings.auto_switch_business_priority,
        plan_multiplier: active_acc.map(|a| a.effective_multiplier()).unwrap_or(1.0),
        accounts: status_entries,
    };
    write_status_file(&status)?;

    // 4. Auto-switch check (only when auto_switch and auto_switch_enabled are true)
    if auto_switch && accounts_file.settings.auto_switch_enabled {
        if let Some(active) = active_acc {
            let threshold = accounts_file.settings.switch_threshold_percent;
            let biz_priority = accounts_file.settings.auto_switch_business_priority;
            if needs_switch(active, threshold, biz_priority, &accounts_file.accounts) {
                if biz_priority && !active.is_business() && active.last_primary_percentage > threshold {
                    println!(
                        "⚡ Active account '{}' is non-business ({:.1}%). Business quota is available. Preempting to business account...",
                        active.id, active.last_primary_percentage
                    );
                } else {
                    println!(
                        "⚠️ Active account '{}' reached {:.1}% (threshold: {:.1}%). Searching for switch candidate...",
                        active.id, active.last_primary_percentage, threshold
                    );
                }

                if let Some(next_id) = select_best_switch(
                    Some(&active.id),
                    &accounts_file.accounts,
                    threshold,
                    &accounts_file.settings.strategy,
                    accounts_file.settings.auto_switch_business_only,
                    accounts_file.settings.auto_switch_business_priority,
                ) {
                    let now = Instant::now();
                    let switch_cooldown = Duration::from_secs(120);
                    let notify_cooldown = Duration::from_secs(300);

                    if should_skip_redundant_switch(
                        last_switched_id.as_deref(),
                        &next_id,
                        accounts_file.active_account_id.as_deref(),
                        *last_switched_time,
                        now,
                        switch_cooldown,
                    ) {
                        println!("ℹ️ Switch to [{}] deferred (recent attempt in cooldown)", next_id);
                    } else {
                        let should_notify = should_notify_switch(
                            last_notified_id.as_deref(),
                            accounts_file.active_account_id.as_deref(),
                            &next_id,
                            accounts_file.settings.notify_on_switch,
                            *last_notified_time,
                            now,
                            notify_cooldown,
                        );
                        println!("🔄 Auto-switching to account '{}'...", next_id);
                        switch_to_account(
                            &next_id,
                            accounts_file.settings.restart_app_on_switch,
                            should_notify,
                        )?;
                        *last_switched_id = Some(next_id.clone());
                        *last_switched_time = Some(now);
                        if should_notify {
                            *last_notified_id = Some(next_id.clone());
                            *last_notified_time = Some(now);
                        }
                        println!("✅ Successfully switched to '{}'!", next_id);
                    }
                } else {
                    println!("⚠️ No alternate account with available quota found.");
                }
            }
        }
    }

    Ok(())
}

/// Runs a single daemon tick with auto-switch enabled (for background daemon loop).
#[allow(dead_code)]
pub fn run_daemon_tick() -> Result<(), String> {
    let mut last_switched_id = None;
    let mut last_switched_time = None;
    let mut last_notified_id = None;
    let mut last_notified_time = None;
    run_daemon_tick_with_state(
        &mut last_switched_id,
        &mut last_switched_time,
        &mut last_notified_id,
        &mut last_notified_time,
        true,
    )
}

/// Refreshes quotas and status without performing auto-switch or restarting apps.
/// Safe for manual status refreshes, UI polling, and post-account-add updates.
pub fn refresh_quotas_and_status() -> Result<(), String> {
    let mut last_switched_id = None;
    let mut last_switched_time = None;
    let mut last_notified_id = None;
    let mut last_notified_time = None;
    run_daemon_tick_with_state(
        &mut last_switched_id,
        &mut last_switched_time,
        &mut last_notified_id,
        &mut last_notified_time,
        false,
    )
}

pub fn run_daemon_loop() {
    println!("🚀 Starting Codex Usage Monitor & Switcher Daemon...");
    let lock_path = daemon_lock_path();
    if let Some(parent) = lock_path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let lock_file = match OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            eprintln!("Failed to open daemon lockfile: {}", e);
            return;
        }
    };
    use fs2::FileExt;
    if lock_file.try_lock_exclusive().is_err() {
        eprintln!(
            "⚠️ Codex switcher daemon is already running (lock held at {}).",
            lock_path.display()
        );
        return;
    }

    let mut last_switched_id = None;
    let mut last_switched_time = None;
    let mut last_notified_id = None;
    let mut last_notified_time = None;
    let mut last_auth_mtime: Option<std::time::SystemTime>;

    loop {
        if let Err(e) = run_daemon_tick_with_state(
            &mut last_switched_id,
            &mut last_switched_time,
            &mut last_notified_id,
            &mut last_notified_time,
            true,
        ) {
            eprintln!("Error in daemon tick: {}", e);
        }

        last_auth_mtime = std::fs::metadata(crate::storage::auth_json_path())
            .and_then(|m| m.modified())
            .ok();

        let interval_secs = load_accounts()
            .map(|a| a.settings.poll_interval_seconds.max(5))
            .unwrap_or(60);

        let sleep_start = Instant::now();
        let target_duration = Duration::from_secs(interval_secs);

        // Responsive sleep: check every 1 second if auth.json was modified externally (e.g. login in Codex app)
        while sleep_start.elapsed() < target_duration {
            sleep(Duration::from_secs(1));
            let current_auth_mtime = std::fs::metadata(crate::storage::auth_json_path())
                .and_then(|m| m.modified())
                .ok();
            if current_auth_mtime != last_auth_mtime && current_auth_mtime.is_some() {
                break;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountConfig, AuthJson, AuthTokens, Settings};

    fn make_test_account(id: &str, email: &str, acc_id: &str, access_tok: &str) -> AccountConfig {
        AccountConfig {
            id: id.to_string(),
            name: None,
            email: email.to_string(),
            plan_type: "team".to_string(),
            account_id: acc_id.to_string(),
            tokens: AuthTokens {
                access_token: access_tok.to_string(),
                refresh_token: Some("rt_old".to_string()),
                id_token: None,
                account_id: Some(acc_id.to_string()),
            },
            enabled: true,
            priority: 0,
            last_primary_percentage: 100.0,
            last_reset_time: None,
            last_reset_after_seconds: None,
            last_weekly_percentage: None,
            last_credits: None,
            last_error: Some("HTTP 401 Unauthorized".to_string()),
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
        }
    }

    #[test]
    fn test_sync_active_tokens_by_account_id() {
        let mut file = AccountsFile {
            active_account_id: None,
            settings: Settings::default(),
            accounts: vec![
                make_test_account("work", "work@company.com", "uuid-work", "tok_work"),
                make_test_account("main", "user@home.com", "uuid-main", "tok_old"),
            ],
        };

        let auth = AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(AuthTokens {
                access_token: "tok_new_fresh".to_string(),
                refresh_token: Some("rt_new".to_string()),
                id_token: None,
                account_id: Some("uuid-main".to_string()),
            }),
            last_refresh: None,
        };

        let changed = sync_active_tokens_from_auth_obj(&mut file, &auth);
        assert!(changed);
        assert_eq!(file.active_account_id.as_deref(), Some("main"));
        assert_eq!(file.accounts[1].tokens.access_token, "tok_new_fresh");
        assert_eq!(file.accounts[1].tokens.refresh_token.as_deref(), Some("rt_new"));
        assert!(file.accounts[1].last_error.is_none());
        assert_eq!(file.accounts[0].tokens.access_token, "tok_work"); // work unchanged
    }

    #[test]
    fn test_sync_active_tokens_already_in_sync() {
        let mut file = AccountsFile {
            active_account_id: Some("main".to_string()),
            settings: Settings::default(),
            accounts: vec![make_test_account("main", "user@home.com", "uuid-main", "tok_current")],
        };

        let auth = AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(AuthTokens {
                access_token: "tok_current".to_string(),
                refresh_token: Some("rt_old".to_string()),
                id_token: None,
                account_id: Some("uuid-main".to_string()),
            }),
            last_refresh: None,
        };

        let changed = sync_active_tokens_from_auth_obj(&mut file, &auth);
        assert!(!changed);
    }

    fn make_test_jwt(email: &str) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let payload = format!(r#"{{"email":"{}"}}"#, email);
        let b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("eyJhbGciOiJub25lIn0.{}.sig", b64)
    }

    #[test]
    fn test_sync_active_tokens_auto_adds_new_external_account() {
        let mut file = AccountsFile {
            active_account_id: Some("dev-alt".to_string()),
            settings: Settings::default(),
            accounts: vec![
                make_test_account("dev-alt", "dev.alt@example.com", "team-uuid", "tok_au"),
            ],
        };

        // User logs in to dev.user@example.com in ChatGPT.app (same team workspace UUID)
        let auth = AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(AuthTokens {
                access_token: "tok_works".to_string(),
                refresh_token: Some("rt_works".to_string()),
                id_token: Some(make_test_jwt("dev.user@example.com")),
                account_id: Some("team-uuid".to_string()),
            }),
            last_refresh: None,
        };

        // Auto-sync must add the new account and NOT overwrite dev-alt
        let changed = sync_active_tokens_from_auth_obj(&mut file, &auth);
        assert!(changed);
        assert_eq!(file.accounts.len(), 2);
        assert_eq!(file.accounts[0].id, "dev.alt@example.com:team-uuid");
        assert_eq!(file.accounts[0].name.as_deref(), Some("dev-alt"));
        assert_eq!(file.accounts[0].email, "dev.alt@example.com");
        assert_eq!(file.accounts[0].tokens.access_token, "tok_au");
        assert_eq!(file.accounts[1].id, "dev.user@example.com:team-uuid");
        assert_eq!(file.accounts[1].name.as_deref(), Some("dev.user"));
        assert_eq!(file.accounts[1].email, "dev.user@example.com");
        assert_eq!(file.accounts[1].tokens.access_token, "tok_works");
        assert_eq!(file.active_account_id.as_deref(), Some("dev.user@example.com:team-uuid"));
    }

    #[test]
    fn test_sync_active_tokens_ignores_empty_or_whitespace_tokens() {
        let mut file = AccountsFile {
            active_account_id: Some("main".to_string()),
            settings: Settings::default(),
            accounts: vec![make_test_account("main", "user@home.com", "uuid-main", "tok_valid")],
        };

        let auth = AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(AuthTokens {
                access_token: "   ".to_string(),
                refresh_token: None,
                id_token: None,
                account_id: None,
            }),
            last_refresh: None,
        };

        let changed = sync_active_tokens_from_auth_obj(&mut file, &auth);
        assert!(!changed);
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(file.accounts[0].tokens.access_token, "tok_valid");
    }

    #[test]
    fn test_sync_active_tokens_does_not_auto_add_placeholder_email() {
        let mut file = AccountsFile {
            active_account_id: Some("main".to_string()),
            settings: Settings::default(),
            accounts: vec![make_test_account("main", "user@home.com", "uuid-main", "tok_valid")],
        };

        // Unknown token without valid email or with placeholder email
        let auth = AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(AuthTokens {
                access_token: "some_new_token_without_email".to_string(),
                refresh_token: None,
                id_token: None,
                account_id: Some("unknown-uuid".to_string()),
            }),
            last_refresh: None,
        };

        let changed = sync_active_tokens_from_auth_obj(&mut file, &auth);
        assert!(!changed);
        assert_eq!(file.accounts.len(), 1);
    }
}
