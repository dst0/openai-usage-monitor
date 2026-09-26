use crate::models::{AccountsFile, AuthJson, AuthTokens};
use crate::oauth::{extract_jwt_metadata, extract_jwt_metadata_from_tokens};
use crate::quota::update_account_quota_cache_with_policy;
use crate::storage::{
    codex_home, compare_and_write_active_auth_json, load_accounts, read_active_auth_json,
};
use std::process::Command;

use super::{
    build_predictable_account_id, deduplicate_accounts_file,
    relogin_commit_service::ReloginCommitService,
    relogin_registry_commit_service::ReloginRegistryCommitService,
    relogin_temp_home::ReloginTempHome, resolve_codex_bin,
};

/// Applies new OAuth tokens to an existing account identified by `target_query`.
/// Stages a validated registry update without writing the shared active auth.
pub fn apply_relogin_to_accounts_file(
    accounts_file: &mut AccountsFile,
    target_query: &str,
    tokens: AuthTokens,
) -> Result<String, String> {
    let target_idx =
        crate::switcher::resolve_target_account_idx(&accounts_file.accounts, target_query)?;
    let original_id = accounts_file.accounts[target_idx].id.clone();
    let was_active = accounts_file.active_account_id.as_deref() == Some(original_id.as_str());
    let target = &mut accounts_file.accounts[target_idx];

    let (_, extracted_plan) = extract_jwt_metadata_from_tokens(&tokens);
    let is_valid_email = |e: &str| -> bool {
        let t = e.trim();
        !t.is_empty()
            && t.contains('@')
            && !t.eq_ignore_ascii_case("user@openai.com")
            && !t.eq_ignore_ascii_case("current-user")
    };
    let id_email = extract_jwt_metadata(tokens.id_token.as_deref()).0;
    let access_email = extract_jwt_metadata(Some(&tokens.access_token)).0;
    for email in [id_email.as_deref(), access_email.as_deref()]
        .into_iter()
        .flatten()
    {
        if !is_valid_email(email) {
            return Err("Re-login JWT contains an unusable email identity".into());
        }
    }
    if id_email
        .as_deref()
        .zip(access_email.as_deref())
        .is_some_and(|(id, access)| !id.trim().eq_ignore_ascii_case(access.trim()))
    {
        return Err("Re-login JWT email claims conflict".into());
    }
    let extracted_email = id_email.or(access_email);

    if let Some(ref email) = extracted_email {
        if is_valid_email(email)
            && is_valid_email(&target.email)
            && !email.trim().eq_ignore_ascii_case(target.email.trim())
        {
            return Err(format!(
                "Logged in as '{}', but expected '{}'. Re-login aborted to protect existing account.",
                email.trim(),
                target.email.trim()
            ));
        }
    }

    let saved_workspace = target
        .tokens
        .account_id
        .as_deref()
        .filter(|id| !id.trim().is_empty() && !id.eq_ignore_ascii_case("default"))
        .unwrap_or(target.account_id.as_str());
    if !saved_workspace.trim().is_empty()
        && !saved_workspace.eq_ignore_ascii_case("default")
        && tokens.account_id.as_deref().map(str::trim) != Some(saved_workspace.trim())
    {
        return Err("Re-login workspace does not match the selected account".into());
    }

    // Workspace IDs can be shared by several people. A browser login for an
    // existing registry entry must prove the same user, even when the
    // workspace happens to match. The official CLI produced these tokens;
    // the local JWT parser only extracts their claim and cannot verify a
    // signature without an issuer key.
    let new_email = extracted_email
        .as_deref()
        .filter(|email| is_valid_email(email))
        .ok_or("Re-login tokens do not contain a usable email identity")?;
    if !is_valid_email(&target.email) {
        return Err("Existing account has no usable email identity for re-login".into());
    }
    if !new_email.trim().eq_ignore_ascii_case(target.email.trim()) {
        return Err(format!(
            "Logged in as '{}', but expected '{}'. Re-login aborted to protect existing account.",
            new_email.trim(),
            target.email.trim()
        ));
    }

    target.email = new_email.trim().to_string();

    if let Some(ref new_plan) = extracted_plan {
        if !new_plan.trim().is_empty() {
            target.plan_type = new_plan.trim().to_string();
        }
    }

    let final_tokens = tokens;
    // A known workspace was required above. A registry entry without one may
    // gain the workspace returned by the new browser login.
    if let Some(ref tok_acc_id) = final_tokens.account_id {
        if !tok_acc_id.trim().is_empty() && tok_acc_id != "default" {
            target.account_id = tok_acc_id.trim().to_string();
        }
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

    if was_active {
        accounts_file.active_account_id = Some(updated_id.clone());
    }

    deduplicate_accounts_file(accounts_file);
    Ok(updated_id)
}

/// Re-authenticates an existing account via browser login, preserving settings and verifying email match.
pub fn relogin_account(query: &str, restart: bool, _no_restart: bool) -> Result<(), String> {
    if restart {
        return Err("Re-login --restart is unavailable while Desktop credential handoff is unverified; close ChatGPT before re-login and open it after success".into());
    }
    let accounts_file = load_accounts()?;
    let target_idx = crate::switcher::resolve_target_account_idx(&accounts_file.accounts, query)?;
    let target_account = accounts_file.accounts[target_idx].clone();
    if accounts_file.active_account_id.as_deref() == Some(target_account.id.as_str())
        && crate::switcher::is_shared_auth_active_checked()?
    {
        return Err(
            "ChatGPT is using shared credentials; close it before re-login of the active account"
                .into(),
        );
    }

    println!(
        "🌐 Launching Codex login in browser for account '{}' ({})...",
        target_account.display_name(),
        target_account.email
    );

    // Browser login uses a private CODEX_HOME and cannot modify the Desktop
    // credential store until the identity-checked commit below.
    let temp_home = ReloginTempHome::new()?;

    // If config.toml exists in real CODEX_HOME, copy it to temp_dir so custom proxy/network settings are honored
    let real_home = codex_home();
    let real_config = real_home.join("config.toml");
    if real_config.exists() {
        let _ = std::fs::copy(&real_config, temp_home.path().join("config.toml"));
    }

    let codex_bin = resolve_codex_bin()?;
    let status = Command::new(&codex_bin)
        .arg("login")
        .env("CODEX_HOME", temp_home.path())
        .status()
        .map_err(|e| format!("Failed to run codex login: {}", e))?;

    if !status.success() {
        return Err("Codex login failed or was cancelled.".to_string());
    }

    let temp_auth_path = temp_home.path().join("auth.json");
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

    let operation = crate::recovery::operation_lock()?;
    let mut accounts_file = load_accounts()?;
    let target_idx = crate::switcher::resolve_target_account_idx(&accounts_file.accounts, query)?;
    let original_account = accounts_file.accounts[target_idx].clone();
    let updated_id = apply_relogin_to_accounts_file(&mut accounts_file, query, tokens)?;

    // Update quota cache for the target account
    if let Some(pos) = accounts_file
        .accounts
        .iter()
        .position(|a| a.id == updated_id)
    {
        let mut account = accounts_file.accounts[pos].clone();
        // Quota inspection must not rotate a fresh login token before the
        // active auth and registry can be committed together.
        update_account_quota_cache_with_policy(&mut account, false);
        accounts_file.accounts[pos] = account;
    }

    ReloginCommitService::new(&original_account, &accounts_file, &updated_id).commit_with(
        crate::switcher::is_shared_auth_active_checked,
        read_active_auth_json,
        |expected, replacement| {
            compare_and_write_active_auth_json(
                expected,
                replacement,
                crate::switcher::is_shared_auth_active_checked,
            )
        },
        |staged, expected_active_auth| {
            ReloginRegistryCommitService::new(&original_account, &updated_id)
                .commit(staged, expected_active_auth)
        },
    )?;
    drop(operation);

    // Refresh quotas and status file so Menu Bar app updates immediately
    let _ = crate::daemon::refresh_quotas_and_status();

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
