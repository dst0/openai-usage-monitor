use crate::models::{AccountsFile, AuthJson, AuthTokens};
use crate::oauth::extract_jwt_metadata_from_tokens;
use crate::quota::update_account_quota_cache;
use crate::storage::{
    codex_home, load_accounts, read_active_auth_json, save_accounts, write_active_auth_json,
};
use std::process::Command;

use super::{build_predictable_account_id, deduplicate_accounts_file, resolve_codex_bin};

/// Applies new OAuth tokens to an existing account identified by `target_query`.
/// Performs strict validation and preserves active-auth synchronization.
pub fn apply_relogin_to_accounts_file(
    accounts_file: &mut AccountsFile,
    target_query: &str,
    tokens: AuthTokens,
) -> Result<String, String> {
    let target_idx =
        crate::switcher::resolve_target_account_idx(&accounts_file.accounts, target_query)?;
    let target = &mut accounts_file.accounts[target_idx];

    let (extracted_email, extracted_plan) = extract_jwt_metadata_from_tokens(&tokens);
    let is_valid_email = |e: &str| -> bool {
        let t = e.trim();
        !t.is_empty()
            && t.contains('@')
            && !t.eq_ignore_ascii_case("user@openai.com")
            && !t.eq_ignore_ascii_case("current-user")
    };

    if let Some(ref new_email) = extracted_email {
        if is_valid_email(new_email) && is_valid_email(&target.email) {
            if !new_email.trim().eq_ignore_ascii_case(target.email.trim()) {
                return Err(format!(
                    "Logged in as '{}', but expected '{}'. Re-login aborted to protect existing account.",
                    new_email.trim(),
                    target.email.trim()
                ));
            }
        }
    }

    if let Some(ref new_email) = extracted_email {
        if is_valid_email(new_email) {
            target.email = new_email.trim().to_string();
        }
    }

    if let Some(ref new_plan) = extracted_plan {
        if !new_plan.trim().is_empty() {
            target.plan_type = new_plan.trim().to_string();
        }
    }

    let mut final_tokens = tokens;
    // If incoming tokens lack account_id (workspace UUID), but target had one, preserve it
    if let Some(ref tok_acc_id) = final_tokens.account_id {
        if !tok_acc_id.trim().is_empty() && tok_acc_id != "default" {
            target.account_id = tok_acc_id.trim().to_string();
        }
    } else if !target.account_id.is_empty() && target.account_id != "default" {
        final_tokens.account_id = Some(target.account_id.clone());
    }

    target.tokens = final_tokens;
    target.enabled = true;
    target.last_error = None; // Clear the error on re-login!

    // Recompute canonical predictable ID
    let is_real_email = is_valid_email(&target.email);
    if is_real_email {
        let canonical_id = build_predictable_account_id(&target.email, &target.account_id);
        target.id = canonical_id;
    }

    let updated_id = target.id.clone();

    // Check if target is the active account in accounts.json
    let is_active = accounts_file
        .active_account_id
        .as_deref()
        .map(|id| id == updated_id)
        .unwrap_or(false);

    if is_active {
        if let Ok(mut live_auth) = read_active_auth_json() {
            live_auth.tokens = Some(target.tokens.clone());
            live_auth.last_refresh = Some(chrono::Utc::now().to_rfc3339());
            let _ = write_active_auth_json(&live_auth);
        }
    }

    deduplicate_accounts_file(accounts_file);
    Ok(updated_id)
}

/// Re-authenticates an existing account via browser login, preserving settings and verifying email match.
pub fn relogin_account(query: &str, restart: bool, no_restart: bool) -> Result<(), String> {
    let mut accounts_file = load_accounts()?;
    let target_idx = crate::switcher::resolve_target_account_idx(&accounts_file.accounts, query)?;
    let target_account = accounts_file.accounts[target_idx].clone();

    println!(
        "🌐 Launching Codex login in browser for account '{}' ({})...",
        target_account.display_name(),
        target_account.email
    );

    // Create a secure, isolated temporary directory for CODEX_HOME
    // so active ~/.codex/auth.json and ChatGPT.app session remain completely untouched during login.
    let temp_dir_name = format!("codex-relogin-{}", std::process::id());
    let temp_dir = std::env::temp_dir().join(temp_dir_name);
    let _ = std::fs::remove_dir_all(&temp_dir);
    std::fs::create_dir_all(&temp_dir)
        .map_err(|e| format!("Failed to create temporary login directory: {}", e))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&temp_dir, std::fs::Permissions::from_mode(0o700));
    }

    struct TempDirGuard<'a>(&'a std::path::Path);
    impl<'a> Drop for TempDirGuard<'a> {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(self.0);
        }
    }
    let _guard = TempDirGuard(&temp_dir);

    // If config.toml exists in real CODEX_HOME, copy it to temp_dir so custom proxy/network settings are honored
    let real_home = codex_home();
    let real_config = real_home.join("config.toml");
    if real_config.exists() {
        let _ = std::fs::copy(&real_config, temp_dir.join("config.toml"));
    }

    let codex_bin = resolve_codex_bin();
    let status = Command::new(&codex_bin)
        .arg("login")
        .env("CODEX_HOME", &temp_dir)
        .status()
        .map_err(|e| format!("Failed to run codex login: {}", e))?;

    if !status.success() {
        return Err("Codex login failed or was cancelled.".to_string());
    }

    let temp_auth_path = temp_dir.join("auth.json");
    if !temp_auth_path.exists() {
        return Err("Login succeeded but no credentials were generated.".to_string());
    }

    let auth_content = std::fs::read_to_string(&temp_auth_path)
        .map_err(|e| format!("Failed to read new auth file: {}", e))?;
    let new_auth: AuthJson = serde_json::from_str(&auth_content)
        .map_err(|e| format!("Failed to parse new auth file: {}", e))?;
    let tokens = new_auth
        .tokens
        .ok_or_else(|| "No tokens found in new auth session".to_string())?;

    let updated_id = apply_relogin_to_accounts_file(&mut accounts_file, query, tokens)?;

    // Update quota cache for the target account
    if let Some(pos) = accounts_file
        .accounts
        .iter()
        .position(|a| a.id == updated_id)
    {
        let mut account = accounts_file.accounts[pos].clone();
        update_account_quota_cache(&mut account);
        accounts_file.accounts[pos] = account;
    }

    save_accounts(&accounts_file)?;

    // Refresh quotas and status file so Menu Bar app updates immediately
    let _ = crate::daemon::refresh_quotas_and_status();

    let is_active = accounts_file
        .active_account_id
        .as_deref()
        .map(|id| id == updated_id)
        .unwrap_or(false);

    let should_restart =
        is_active && (restart || accounts_file.settings.restart_app_on_switch) && !no_restart;
    if should_restart && crate::switcher::is_codex_app_running() {
        println!("🔄 Restarting ChatGPT desktop app to apply updated credentials...");
        let _ = crate::switcher::restart_and_recover(0, None);
    }

    let final_acc = accounts_file.accounts.iter().find(|a| a.id == updated_id);
    let final_display = final_acc
        .map(|a| a.display_name())
        .unwrap_or_else(|| updated_id.as_str());
    let final_email = final_acc.map(|a| a.email.as_str()).unwrap_or("");

    println!(
        "🎉 Successfully re-authenticated account '{}' ({})!",
        final_display, final_email
    );

    Ok(())
}
