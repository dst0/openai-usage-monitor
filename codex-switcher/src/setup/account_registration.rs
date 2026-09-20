use crate::models::{AccountConfig, AccountsFile, AuthTokens};
use crate::oauth::extract_jwt_metadata_from_tokens;
use crate::quota::update_account_quota_cache;
use crate::storage::{load_accounts, read_active_auth_json, save_accounts};

use super::{build_predictable_account_id, deduplicate_accounts_file, find_existing_account_idx};

/// Adds or updates an account in AccountsFile in memory from tokens and metadata.
/// Computes a predictable ID from <email>:<account_id> and saves optional nickname in `name`.
pub fn add_account_to_accounts_file(
    accounts_file: &mut AccountsFile,
    nickname_or_id: &str,
    tokens: AuthTokens,
    preserve_active: bool,
) -> String {
    let (email, plan) = extract_jwt_metadata_from_tokens(&tokens);
    let email_val = email.unwrap_or_else(|| "user@openai.com".to_string());
    let plan_val = plan.unwrap_or_else(|| "team".to_string());
    let acc_id_val = tokens
        .account_id
        .clone()
        .unwrap_or_else(|| "default".to_string());
    let canonical_id = build_predictable_account_id(&email_val, &acc_id_val);
    let nick_trimmed = nickname_or_id.trim();

    // 1. Identify existing account by canonical_id, nickname, token, or account_id
    let existing_idx = find_existing_account_idx(
        &accounts_file.accounts,
        &canonical_id,
        &email_val,
        &acc_id_val,
        Some(&plan_val),
        Some(&tokens),
    )
    .or_else(|| {
        if !nick_trimmed.is_empty() {
            find_existing_account_idx(
                &accounts_file.accounts,
                nick_trimmed,
                &email_val,
                &acc_id_val,
                Some(&plan_val),
                Some(&tokens),
            )
        } else {
            None
        }
    });

    let target_id = if let Some(idx) = existing_idx {
        let existing = &mut accounts_file.accounts[idx];
        existing.id = canonical_id.clone();
        if !nick_trimmed.is_empty()
            && !nick_trimmed.contains(':')
            && !nick_trimmed.eq_ignore_ascii_case(&email_val)
        {
            existing.name = Some(nick_trimmed.to_string());
        }
        existing.tokens = tokens;
        existing.email = email_val;
        existing.plan_type = plan_val;
        existing.account_id = acc_id_val;
        existing.enabled = true;
        canonical_id
    } else {
        let resolved_name = if !nick_trimmed.is_empty()
            && !nick_trimmed.contains(':')
            && !nick_trimmed.eq_ignore_ascii_case(&email_val)
        {
            Some(nick_trimmed.to_string())
        } else {
            let prefix = email_val.split('@').next().unwrap_or("account").to_string();
            Some(prefix)
        };

        let acc = AccountConfig {
            id: canonical_id.clone(),
            name: resolved_name,
            email: email_val,
            plan_type: plan_val,
            account_id: acc_id_val,
            tokens,
            enabled: true,
            priority: (accounts_file.accounts.len() + 1) as i32,
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
        };
        accounts_file.accounts.push(acc);
        canonical_id
    };

    let active_valid = accounts_file
        .active_account_id
        .as_ref()
        .map(|act| accounts_file.accounts.iter().any(|a| a.id == *act))
        .unwrap_or(false);

    if !preserve_active || !active_valid {
        accounts_file.active_account_id = Some(target_id.clone());
    }

    deduplicate_accounts_file(accounts_file);
    target_id
}

/// Adds or updates an account, updates quota cache, and saves to accounts.json.
pub fn add_account_from_tokens(
    accounts_file: &mut AccountsFile,
    id: &str,
    tokens: AuthTokens,
    preserve_active: bool,
) -> Result<String, String> {
    let target_id = add_account_to_accounts_file(accounts_file, id, tokens, preserve_active);

    // Update quota cache for the target account
    if let Some(pos) = accounts_file
        .accounts
        .iter()
        .position(|a| a.id == target_id)
    {
        let mut account = accounts_file.accounts[pos].clone();
        update_account_quota_cache(&mut account);
        accounts_file.accounts[pos] = account;
    }

    save_accounts(accounts_file)?;
    Ok(target_id)
}

pub fn save_current_as(id: &str) -> Result<(), String> {
    let auth = read_active_auth_json()?;
    let tokens = auth
        .tokens
        .ok_or_else(|| "No tokens found in auth.json".to_string())?;

    let mut accounts_file = load_accounts().unwrap_or_default();
    let target_id = add_account_from_tokens(&mut accounts_file, id, tokens, false)?;
    let saved_email = accounts_file
        .accounts
        .iter()
        .find(|a| a.id == target_id)
        .map(|a| a.email.as_str())
        .unwrap_or("");
    println!(
        "✅ Current session saved as '{}' ({})",
        target_id, saved_email
    );
    Ok(())
}

pub fn remove_account(id: &str) -> Result<(), String> {
    let mut accounts_file = load_accounts()?;
    let orig_len = accounts_file.accounts.len();
    let id_trimmed = id.trim();

    // Match by exact canonical ID, nickname, email, or workspace UUID
    let has_id_match = accounts_file
        .accounts
        .iter()
        .any(|a| a.id.trim().eq_ignore_ascii_case(id_trimmed));
    if has_id_match {
        accounts_file
            .accounts
            .retain(|a| !a.id.trim().eq_ignore_ascii_case(id_trimmed));
    } else {
        let has_name_match = accounts_file.accounts.iter().any(|a| {
            a.name
                .as_deref()
                .map(|n| n.trim().eq_ignore_ascii_case(id_trimmed))
                == Some(true)
        });
        if has_name_match {
            accounts_file.accounts.retain(|a| {
                a.name
                    .as_deref()
                    .map(|n| n.trim().eq_ignore_ascii_case(id_trimmed))
                    != Some(true)
            });
        } else {
            let has_email_match = accounts_file
                .accounts
                .iter()
                .any(|a| a.email.trim().eq_ignore_ascii_case(id_trimmed));
            if has_email_match {
                accounts_file
                    .accounts
                    .retain(|a| !a.email.trim().eq_ignore_ascii_case(id_trimmed));
            } else {
                accounts_file
                    .accounts
                    .retain(|a| !a.account_id.trim().eq_ignore_ascii_case(id_trimmed));
            }
        }
    }

    if accounts_file.accounts.len() == orig_len {
        return Err(format!("Account '{}' not found", id));
    }

    if accounts_file.active_account_id.as_deref() == Some(id_trimmed)
        || !accounts_file
            .accounts
            .iter()
            .any(|a| Some(&a.id) == accounts_file.active_account_id.as_ref())
    {
        accounts_file.active_account_id = accounts_file.accounts.first().map(|a| a.id.clone());
    }

    save_accounts(&accounts_file)?;
    println!("✅ Account '{}' removed.", id);
    Ok(())
}
