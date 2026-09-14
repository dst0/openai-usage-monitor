use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens};
use crate::oauth::extract_jwt_metadata_from_tokens;
use crate::quota::update_account_quota_cache;
use crate::storage::{
    codex_home, load_accounts, read_active_auth_json, save_accounts, sync_settings_to_status_file,
    write_active_auth_json,
};
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::process::Command;

/// Builds a predictable, deterministic account ID from email and ChatGPT account_id (workspace UUID).
/// Example: "user@example.com:3f533057-4bac-44ea-a999-5bb5748ca9cb"
pub fn build_predictable_account_id(email: &str, account_id: &str) -> String {
    let email_clean = email.trim().to_lowercase();
    let acc_id_clean = account_id.trim();
    if acc_id_clean.is_empty() || acc_id_clean.eq_ignore_ascii_case("default") {
        format!("{}:default", email_clean)
    } else {
        format!("{}:{}", email_clean, acc_id_clean)
    }
}

/// Helper to identify whether an existing account matches by:
/// 1. Exact canonical ID or alias match
/// 2. Exact nickname match
/// 3. Exact refresh_token or access_token match
/// 4. Canonical predictable ID match
/// 5. Exact ChatGPT account_id match (if non-empty and != "default")
/// 6. Case-insensitive email match (if non-empty and unambiguous)
pub fn find_existing_account_idx_from_parts(
    accounts: &[AccountConfig],
    id: &str,
    email: &str,
    account_id: &str,
    plan: Option<&str>,
    refresh_token: Option<&str>,
    access_token: Option<&str>,
) -> Option<usize> {
    let email_trimmed = email.trim();
    let is_valid_email = |e: &str| -> bool {
        let t = e.trim();
        !t.is_empty()
            && t.contains('@')
            && !t.eq_ignore_ascii_case("user@openai.com")
            && !t.eq_ignore_ascii_case("current-user")
    };
    let has_real_email = is_valid_email(email_trimmed);
    let id_trimmed = id.trim();
    let acc_id_trimmed = account_id.trim();
    let has_acc_id = !acc_id_trimmed.is_empty() && acc_id_trimmed != "default";
    let rt_trimmed = refresh_token.map(str::trim).unwrap_or("");
    let at_trimmed = access_token.map(str::trim).unwrap_or("");
    let incoming_plan = plan.map(str::trim).unwrap_or("");

    // Helper: check if incoming email and candidate account email are in direct conflict
    let has_email_conflict = |candidate: &AccountConfig| -> bool {
        let cand_email = candidate.email.trim();
        let cand_has_real = is_valid_email(cand_email);
        has_real_email && cand_has_real && !cand_email.eq_ignore_ascii_case(email_trimmed)
    };

    // Helper: check if candidate has a known workspace ID that conflicts with incoming workspace ID
    let has_workspace_conflict = |candidate: &AccountConfig| -> bool {
        if !has_acc_id {
            return false;
        }
        let cand_tok_acc_id = candidate
            .tokens
            .account_id
            .as_deref()
            .map(str::trim)
            .unwrap_or("");
        let cand_acc_id = if !cand_tok_acc_id.is_empty() && cand_tok_acc_id != "default" {
            cand_tok_acc_id
        } else {
            candidate.account_id.trim()
        };
        let cand_has_acc_id = !cand_acc_id.is_empty() && cand_acc_id != "default";
        cand_has_acc_id && cand_acc_id != acc_id_trimmed
    };

    // Helper: check if candidate has a known plan that conflicts with incoming plan
    let has_plan_conflict = |candidate: &AccountConfig| -> bool {
        let cand_plan = candidate.plan_type.trim();
        !incoming_plan.is_empty()
            && !cand_plan.is_empty()
            && !cand_plan.eq_ignore_ascii_case(incoming_plan)
    };

    // Helper: check if candidate has a distinct ID that would be clobbered
    let has_id_conflict = |candidate: &AccountConfig| -> bool {
        if id_trimmed.is_empty() || candidate.id.trim().is_empty() {
            return false;
        }
        // "main" is allowed to be renamed to a descriptive name
        if candidate.id.trim().eq_ignore_ascii_case("main") {
            return false;
        }
        let cand_id = candidate.id.trim();
        let cand_name = candidate.name.as_deref().map(str::trim).unwrap_or("");
        !cand_id.eq_ignore_ascii_case(id_trimmed) && !cand_name.eq_ignore_ascii_case(id_trimmed)
    };

    // --- CASE 1: Explicit ID or Nickname provided by caller/user ---
    if !id_trimmed.is_empty() {
        // 1. Exact ID match
        if let Some(pos) = accounts.iter().position(|a| {
            if has_email_conflict(a) {
                return false;
            }
            a.id.trim().eq_ignore_ascii_case(id_trimmed)
        }) {
            return Some(pos);
        }

        // 2. Exact nickname match
        if let Some(pos) = accounts.iter().position(|a| {
            if has_email_conflict(a) || has_workspace_conflict(a) || has_plan_conflict(a) {
                return false;
            }
            a.name
                .as_deref()
                .map(|n| n.trim().eq_ignore_ascii_case(id_trimmed))
                == Some(true)
        }) {
            return Some(pos);
        }

        // 3. Same email and workspace UUID match (or canonical ID match)
        if has_real_email && has_acc_id {
            let canonical_id = build_predictable_account_id(email_trimmed, acc_id_trimmed);
            if let Some(pos) = accounts.iter().position(|a| {
                if has_email_conflict(a) || has_workspace_conflict(a) || has_plan_conflict(a) {
                    return false;
                }
                a.id.trim().eq_ignore_ascii_case(&canonical_id)
                    || (a.email.trim().eq_ignore_ascii_case(email_trimmed)
                        && (a.account_id.trim() == acc_id_trimmed
                            || a.tokens.account_id.as_deref().map(str::trim)
                                == Some(acc_id_trimmed)))
            }) {
                return Some(pos);
            }
        }

        // 4. Token match (same session credentials being updated)
        if !rt_trimmed.is_empty() {
            if let Some(pos) = accounts
                .iter()
                .position(|a| a.tokens.refresh_token.as_deref().map(str::trim) == Some(rt_trimmed))
            {
                return Some(pos);
            }
        }
        if !at_trimmed.is_empty() {
            if let Some(pos) = accounts
                .iter()
                .position(|a| a.tokens.access_token.trim() == at_trimmed)
            {
                return Some(pos);
            }
        }

        return None;
    }

    // --- CASE 2: No explicit ID provided (auto-sync / deduplication) ---

    // Tier 1: Refresh token match across all accounts
    if !rt_trimmed.is_empty() {
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.tokens.refresh_token.as_deref().map(str::trim) == Some(rt_trimmed))
        {
            return Some(pos);
        }
    }

    // Tier 2: Access token match across all accounts
    if !at_trimmed.is_empty() {
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.tokens.access_token.trim() == at_trimmed)
        {
            return Some(pos);
        }
    }

    // Tier 2.5: Canonical predictable ID match (email + workspace UUID)
    if has_real_email && has_acc_id {
        let canonical_id = build_predictable_account_id(email_trimmed, acc_id_trimmed);
        if let Some(pos) = accounts
            .iter()
            .position(|a| a.id.trim().eq_ignore_ascii_case(&canonical_id))
        {
            return Some(pos);
        }
    }

    // Tier 3: ChatGPT Account ID (Workspace UUID) match
    if has_acc_id {
        let matching_candidates: Vec<usize> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if has_email_conflict(a)
                    || has_workspace_conflict(a)
                    || has_plan_conflict(a)
                    || has_id_conflict(a)
                {
                    return false;
                }
                a.account_id.trim() == acc_id_trimmed
                    || a.tokens.account_id.as_deref().map(str::trim) == Some(acc_id_trimmed)
            })
            .map(|(idx, _)| idx)
            .collect();

        if matching_candidates.len() == 1 {
            return Some(matching_candidates[0]);
        }
    }

    // Tier 4: Unambiguous email match
    if has_real_email {
        let email_candidates: Vec<usize> = accounts
            .iter()
            .enumerate()
            .filter(|(_, a)| {
                if has_workspace_conflict(a) || has_plan_conflict(a) || has_id_conflict(a) {
                    return false;
                }
                a.email.trim().eq_ignore_ascii_case(email_trimmed)
            })
            .map(|(idx, _)| idx)
            .collect();

        if email_candidates.len() == 1 {
            return Some(email_candidates[0]);
        }
    }

    None
}

pub fn find_existing_account_idx(
    accounts: &[AccountConfig],
    id: &str,
    email: &str,
    account_id: &str,
    plan: Option<&str>,
    tokens: Option<&AuthTokens>,
) -> Option<usize> {
    let effective_acc_id = if !account_id.trim().is_empty() && account_id.trim() != "default" {
        account_id
    } else if let Some(t_acc) = tokens.and_then(|t| t.account_id.as_deref()) {
        t_acc
    } else {
        account_id
    };

    let mut resolved_plan = plan;
    let extracted_plan;
    if resolved_plan.is_none() {
        if let Some(t) = tokens {
            extracted_plan = crate::oauth::extract_jwt_metadata_from_tokens(t).1;
            resolved_plan = extracted_plan.as_deref();
        }
    }

    find_existing_account_idx_from_parts(
        accounts,
        id,
        email,
        effective_acc_id,
        resolved_plan,
        tokens.and_then(|t| t.refresh_token.as_deref()),
        tokens.map(|t| t.access_token.as_str()),
    )
}

/// Deduplicates accounts in-place, migrating legacy IDs to predictable format (<email>:<account_id>),
/// preserving nicknames in `name`, and merging duplicates by matching token, account_id, or canonical ID.
/// Returns a map of removed/remapped IDs -> survivor canonical IDs.
pub fn deduplicate_accounts(accounts: &mut Vec<AccountConfig>) -> HashMap<String, String> {
    let mut deduped: Vec<AccountConfig> = Vec::with_capacity(accounts.len());
    let mut merged_ids = HashMap::new();

    for mut acc in accounts.drain(..) {
        // Automatically migrate account ID to predictable canonical format (<email>:<account_id>)
        let is_real_email = acc.email.contains('@')
            && !acc.email.eq_ignore_ascii_case("user@openai.com")
            && !acc.email.eq_ignore_ascii_case("current-user");

        if is_real_email {
            let canonical_id = build_predictable_account_id(&acc.email, &acc.account_id);
            if acc.id != canonical_id {
                // If previous ID was a custom label or nickname, migrate it to name!
                if acc.name.is_none() || acc.name.as_deref().map(str::trim) == Some("") {
                    if acc.id.contains("-[personal]") {
                        acc.name = Some("personal".to_string());
                    } else if acc.id.contains("-[business]") {
                        acc.name = Some("business".to_string());
                    } else if !acc.id.contains(':') && !acc.id.eq_ignore_ascii_case(&acc.email) {
                        acc.name = Some(acc.id.clone());
                    } else {
                        let prefix = acc.email.split('@').next().unwrap_or("account").to_string();
                        acc.name = Some(prefix);
                    }
                }
                merged_ids.insert(acc.id.clone(), canonical_id.clone());
                acc.id = canonical_id;
            }
        }

        // Check if an existing account matches by tokens, canonical ID, or workspace
        if let Some(pos) = find_existing_account_idx(
            &deduped,
            "",
            &acc.email,
            &acc.account_id,
            Some(&acc.plan_type),
            Some(&acc.tokens),
        ) {
            let existing = &mut deduped[pos];
            if acc.id != existing.id {
                merged_ids.insert(acc.id.clone(), existing.id.clone());
            }

            // Prefer descriptive nickname over "main" or empty
            let existing_is_placeholder = existing.name.is_none()
                || existing.name.as_deref() == Some("")
                || existing.name.as_deref() == Some("main");
            if existing_is_placeholder && acc.name.is_some() && acc.name.as_deref() != Some("main")
            {
                existing.name = acc.name;
            }

            // Ensure existing has canonical predictable ID
            if is_real_email {
                let canonical_id =
                    build_predictable_account_id(&existing.email, &existing.account_id);
                if existing.id != canonical_id {
                    merged_ids.insert(existing.id.clone(), canonical_id.clone());
                    existing.id = canonical_id;
                }
            }

            // Preserve newest valid tokens
            if !acc.tokens.access_token.trim().is_empty() {
                existing.tokens.access_token = acc.tokens.access_token;
            }
            if let Some(ref rt) = acc.tokens.refresh_token {
                if !rt.trim().is_empty() {
                    existing.tokens.refresh_token = Some(rt.clone());
                }
            }
            if let Some(ref idt) = acc.tokens.id_token {
                if !idt.trim().is_empty() {
                    existing.tokens.id_token = Some(idt.clone());
                }
            }
            if let Some(ref acc_id) = acc.tokens.account_id {
                if !acc_id.trim().is_empty() && acc_id != "default" {
                    existing.tokens.account_id = Some(acc_id.clone());
                }
            }

            // Prefer real email over placeholder
            if acc.email.contains('@')
                && (!existing.email.contains('@')
                    || existing.email == "user@openai.com"
                    || existing.email == "current-user")
            {
                existing.email = acc.email;
            }

            // Upgrade default account_id
            if !acc.account_id.is_empty()
                && acc.account_id != "default"
                && (existing.account_id.is_empty() || existing.account_id == "default")
            {
                existing.account_id = acc.account_id;
            }

            // Upgrade plan type if incoming is more specific
            if existing.plan_type.is_empty()
                || (existing.plan_type == "team"
                    && acc.plan_type != "team"
                    && !acc.plan_type.is_empty())
            {
                existing.plan_type = acc.plan_type;
            }

            // Keep the most recent quota check
            if acc.last_checked > existing.last_checked {
                existing.last_primary_percentage = acc.last_primary_percentage;
                existing.last_reset_time = acc.last_reset_time;
                existing.last_reset_after_seconds = acc.last_reset_after_seconds;
                existing.last_weekly_percentage = acc.last_weekly_percentage;
                existing.last_weekly_reset_time = acc.last_weekly_reset_time;
                existing.last_weekly_reset_after_seconds = acc.last_weekly_reset_after_seconds;
                existing.last_credits = acc.last_credits;
                existing.last_error = acc.last_error;
                existing.last_checked = acc.last_checked;
            }

            if existing.organization_name.is_none() && acc.organization_name.is_some() {
                existing.organization_name = acc.organization_name;
            }

            existing.enabled = existing.enabled || acc.enabled;
        } else {
            deduped.push(acc);
        }
    }

    *accounts = deduped;
    merged_ids
}

/// Deduplicates all accounts in AccountsFile, preserving and remapping active_account_id.
/// Returns true if any changes occurred.
pub fn deduplicate_accounts_file(file: &mut AccountsFile) -> bool {
    let orig_count = file.accounts.len();
    let merged_ids = deduplicate_accounts(&mut file.accounts);
    let mut changed = file.accounts.len() < orig_count || !merged_ids.is_empty();

    if let Some(ref active_id) = file.active_account_id {
        if let Some(new_id) = merged_ids.get(active_id) {
            file.active_account_id = Some(new_id.clone());
            changed = true;
        } else if !file.accounts.iter().any(|a| a.id == *active_id) {
            file.active_account_id = file.accounts.first().map(|a| a.id.clone());
            changed = true;
        }
    } else if !file.accounts.is_empty() {
        file.active_account_id = file.accounts.first().map(|a| a.id.clone());
        changed = true;
    }

    changed
}

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

pub fn run_interactive_setup() -> Result<(), String> {
    println!("==================================================");
    println!("🤖 OpenAI Codex Multi-Account Setup");
    println!("==================================================");

    let accounts_file = load_accounts().unwrap_or_default();
    println!(
        "Current configured accounts: {}",
        accounts_file.accounts.len()
    );
    for acc in &accounts_file.accounts {
        let active = if accounts_file.active_account_id.as_deref() == Some(&acc.id) {
            " (active)"
        } else {
            ""
        };
        println!(
            "  - {} <{}> [{}]{}",
            acc.display_name(),
            acc.email,
            acc.plan_type,
            active
        );
    }
    println!("--------------------------------------------------");
    println!("1. Save current session as a new/updated account");
    println!("2. Add second/next account via Codex CLI login");
    println!("3. Test API quotas for all accounts now");
    println!("4. Exit");
    print!("Select [1-4]: ");
    io::stdout().flush().unwrap();

    let mut line = String::new();
    let stdin = io::stdin();
    stdin
        .lock()
        .read_line(&mut line)
        .map_err(|e| e.to_string())?;

    match line.trim() {
        "1" => {
            print!("Enter a nickname for this account (e.g. personal, work): ");
            io::stdout().flush().unwrap();
            let mut id = String::new();
            stdin.lock().read_line(&mut id).map_err(|e| e.to_string())?;
            let id = id.trim();
            save_current_as(id)?;
        }
        "2" => {
            print!("Enter a nickname for the new account (e.g. personal, work): ");
            io::stdout().flush().unwrap();
            let mut id = String::new();
            stdin.lock().read_line(&mut id).map_err(|e| e.to_string())?;
            let id = id.trim();
            login_and_add_account(id)?;
        }
        "3" => {
            crate::daemon::refresh_quotas_and_status()?;
        }
        _ => {
            println!("Exited.");
        }
    }

    Ok(())
}

pub fn resolve_codex_bin() -> String {
    let app_codex = "/Applications/ChatGPT.app/Contents/Resources/codex";
    if std::path::Path::new(app_codex).exists() {
        return app_codex.to_string();
    }
    "codex".to_string()
}

pub fn login_and_add_account(id: &str) -> Result<(), String> {
    let id_trimmed = id.trim();
    if id_trimmed.is_empty() {
        println!("🌐 Launching Codex login in browser...");
    } else {
        println!(
            "🌐 Launching Codex login in browser for account '{}'...",
            id_trimmed
        );
    }

    // 1. Create a secure, isolated temporary directory for CODEX_HOME
    // so that the active ~/.codex/auth.json and ChatGPT.app session remain completely untouched.
    let temp_dir_name = format!("codex-login-{}", std::process::id());
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

    // If config.toml exists in real CODEX_HOME, copy it to temp_dir so custom network/proxy settings are honored
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

    let mut accounts_file = load_accounts().unwrap_or_default();

    // If accounts_file has no accounts configured, but ~/.codex/auth.json has active credentials,
    // auto-save the active session first so the original account isn't lost!
    if accounts_file.accounts.is_empty() {
        if let Ok(active_auth) = read_active_auth_json() {
            if let Some(active_tokens) = active_auth.tokens {
                let _ = add_account_from_tokens(&mut accounts_file, "", active_tokens, false);
            }
        }
    }

    let had_active_account = accounts_file
        .active_account_id
        .as_ref()
        .map(|act| accounts_file.accounts.iter().any(|a| a.id == *act))
        .unwrap_or(false);

    let target_id = add_account_from_tokens(&mut accounts_file, id_trimmed, tokens.clone(), true)?;

    // If there was no active account previously, initialize ~/.codex/auth.json with the new tokens
    if !had_active_account {
        if let Ok(mut current_auth) = read_active_auth_json() {
            if current_auth.tokens.is_none() {
                current_auth.tokens = Some(tokens);
                let _ = write_active_auth_json(&current_auth);
            }
        } else {
            let new_live_auth = AuthJson {
                auth_mode: Some("chatgpt".to_string()),
                openai_api_key: None,
                tokens: Some(tokens),
                last_refresh: Some(chrono::Utc::now().to_rfc3339()),
            };
            let _ = write_active_auth_json(&new_live_auth);
        }
    }

    // Refresh quotas and status file without auto-switching or app restarts
    let _ = crate::daemon::refresh_quotas_and_status();

    let saved_email = accounts_file
        .accounts
        .iter()
        .find(|a| a.id == target_id)
        .map(|a| a.email.as_str())
        .unwrap_or("");
    println!(
        "🎉 Successfully logged in and added account '{}' ({})! Active account preserved.",
        target_id, saved_email
    );

    Ok(())
}

/// Applies new OAuth tokens to an existing account identified by `target_query`.
/// Performs strict validation:
/// - Resolves target account index.
/// - Extracts email from JWT tokens; if email is present and both are real emails,
///   enforces case-insensitive match to prevent accidental account overwrites.
/// - Preserves existing workspace UUID if not provided by browser session.
/// - Re-evaluates canonical predictable ID (<email>:<account_id>).
/// - Updates tokens, clears `last_error`, sets `enabled = true`.
/// - If target account is active, atomically syncs `~/.codex/auth.json`.
/// Returns the updated canonical account ID.
pub fn apply_relogin_to_accounts_file(
    accounts_file: &mut AccountsFile,
    target_query: &str,
    tokens: AuthTokens,
) -> Result<String, String> {
    let target_idx = crate::switcher::resolve_target_account_idx(&accounts_file.accounts, target_query)?;
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

    let final_acc = accounts_file
        .accounts
        .iter()
        .find(|a| a.id == updated_id);
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

/// Renames an account's display nickname, or clears it if new_name is None.
/// Guarantees that nicknames are not duplicated across different accounts.
pub fn rename_account(query: &str, new_name: Option<&str>) -> Result<(), String> {
    let mut file = load_accounts()?;
    let idx = crate::switcher::resolve_target_account_idx(&file.accounts, query)?;

    let clean_new = new_name.map(str::trim).filter(|s| !s.is_empty());

    // Check for duplicate nicknames across other accounts
    if let Some(name) = clean_new {
        for (i, a) in file.accounts.iter().enumerate() {
            if i != idx {
                if let Some(existing_name) = &a.name {
                    if existing_name.trim().eq_ignore_ascii_case(name) {
                        return Err(format!(
                            "Nickname '{}' is already used by another account ({})",
                            name, a.email
                        ));
                    }
                }
            }
        }
    }

    let acc = &mut file.accounts[idx];
    acc.name = clean_new.map(str::to_string);
    let updated_id = acc.id.clone();
    let updated_name = acc.name.clone();

    save_accounts(&file)?;

    // Refresh quotas and status file so Menu Bar app updates immediately
    let _ = crate::daemon::refresh_quotas_and_status();

    if let Some(name) = updated_name {
        println!("✅ Account '{}' renamed to '{}'", updated_id, name);
    } else {
        println!("✅ Cleared nickname for account '{}'", updated_id);
    }

    Ok(())
}

/// Updates the restart_app_on_switch setting in accounts.json.
pub fn set_config_restart_app_on_switch(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.restart_app_on_switch = enabled;
    save_accounts(&file)?;
    println!("✅ Setting updated: restart_app_on_switch = {}", enabled);
    Ok(())
}

/// Updates the auto_switch_enabled setting in accounts.json.
pub fn set_config_auto_switch_enabled(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.auto_switch_enabled = enabled;
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!("✅ Setting updated: auto_switch_enabled = {}", enabled);
    Ok(())
}

/// Updates the auto_switch_business_only setting in accounts.json.
pub fn set_config_auto_switch_business_only(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.auto_switch_business_only = enabled;
    if enabled {
        file.settings.auto_switch_business_priority = false;
        file.settings.auto_switch_enabled = true;
    }
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!(
        "✅ Setting updated: auto_switch_business_only = {}",
        enabled
    );
    Ok(())
}

/// Updates the auto_switch_business_priority setting in accounts.json.
pub fn set_config_auto_switch_business_priority(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.auto_switch_business_priority = enabled;
    if enabled {
        file.settings.auto_switch_business_only = false;
        file.settings.auto_switch_enabled = true;
    }
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!(
        "✅ Setting updated: auto_switch_business_priority = {}",
        enabled
    );
    Ok(())
}

/// Updates the opt-in weekly reset-credit policy. A threshold of zero means
/// that the policy may act whenever the weekly pool is exactly exhausted;
/// non-zero thresholds require that many seconds to remain before the normal
/// weekly reset. Keeping this as one atomic accounts.json write prevents the
/// daemon from observing a half-updated policy when the menu changes both
/// values together.
pub fn set_config_auto_reset_weekly(
    enabled: bool,
    min_remaining_seconds: u64,
) -> Result<(), String> {
    const MAX_REMAINING_SECONDS: u64 = 167 * 3600;
    if min_remaining_seconds > MAX_REMAINING_SECONDS {
        return Err(format!(
            "Weekly reset threshold must be between 0 and 167 hours (got {})",
            min_remaining_seconds / 3600
        ));
    }
    let mut file = load_accounts()?;
    file.settings.auto_reset_weekly_enabled = enabled;
    file.settings.auto_reset_weekly_min_remaining_seconds = min_remaining_seconds;
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!(
        "✅ Setting updated: auto_reset_weekly_enabled = {}, auto_reset_weekly_min_remaining_seconds = {}",
        enabled, min_remaining_seconds
    );
    Ok(())
}

/// Manually sets a multiplier override for an account.
pub fn set_account_multiplier(account_id: &str, multiplier: f64) -> Result<(), String> {
    if multiplier <= 0.0 {
        return Err("Multiplier must be greater than 0".to_string());
    }
    let mut file = load_accounts()?;
    let acc = file
        .accounts
        .iter_mut()
        .find(|a| {
            a.id == account_id
                || a.name
                    .as_deref()
                    .map(|n| n.eq_ignore_ascii_case(account_id))
                    .unwrap_or(false)
                || a.email.eq_ignore_ascii_case(account_id)
        })
        .ok_or_else(|| format!("Account '{}' not found", account_id))?;

    acc.plan_multiplier = Some(multiplier);
    acc.multiplier_is_manual = Some(true);
    let name = acc.display_name().to_string();
    save_accounts(&file)?;
    println!(
        "✅ Account '{}' multiplier manually set to {:.1}x",
        name, multiplier
    );
    Ok(())
}

/// Clears manual multiplier override and re-runs auto-detection.
pub fn reset_account_multiplier(account_id: &str) -> Result<(), String> {
    let mut file = load_accounts()?;
    let acc = file
        .accounts
        .iter_mut()
        .find(|a| {
            a.id == account_id
                || a.name
                    .as_deref()
                    .map(|n| n.eq_ignore_ascii_case(account_id))
                    .unwrap_or(false)
                || a.email.eq_ignore_ascii_case(account_id)
        })
        .ok_or_else(|| format!("Account '{}' not found", account_id))?;

    acc.multiplier_is_manual = None;
    acc.plan_multiplier = None;
    acc.last_multiplier_checked = None;
    let detected = crate::quota::detect_account_multiplier(acc);
    let name = acc.display_name().to_string();
    save_accounts(&file)?;
    println!(
        "✅ Account '{}' multiplier reset and auto-detected as {:.1}x",
        name, detected
    );
    Ok(())
}

#[cfg(test)]
pub(crate) static TEST_CODEX_HOME_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::{AccountConfig, AuthTokens};

    fn make_test_account(
        id: &str,
        email: &str,
        acc_id: &str,
        refresh_token: Option<&str>,
        access_token: &str,
    ) -> AccountConfig {
        AccountConfig {
            id: id.to_string(),
            name: None,
            email: email.to_string(),
            plan_type: "team".to_string(),
            account_id: acc_id.to_string(),
            tokens: AuthTokens {
                access_token: access_token.to_string(),
                refresh_token: refresh_token.map(String::from),
                id_token: None,
                account_id: Some(acc_id.to_string()),
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
        }
    }

    #[test]
    fn test_build_predictable_account_id() {
        assert_eq!(
            build_predictable_account_id("Dev.User@example.com ", " 3f533057-4bac-44ea "),
            "dev.user@example.com:3f533057-4bac-44ea"
        );
        assert_eq!(
            build_predictable_account_id("foo@bar.com", "default"),
            "foo@bar.com:default"
        );
        assert_eq!(
            build_predictable_account_id("foo@bar.com", ""),
            "foo@bar.com:default"
        );
    }

    #[test]
    fn test_find_existing_account_idx_multi_vector() {
        let accounts = vec![
            make_test_account(
                "main",
                "dev.user@example.com",
                "uuid-1",
                Some("rt_1"),
                "at_1",
            ),
            make_test_account("work", "work@company.com", "uuid-2", Some("rt_2"), "at_2"),
        ];

        // 1. Match by refresh token
        assert_eq!(
            find_existing_account_idx_from_parts(
                &accounts,
                "other",
                "other@foo.com",
                "uuid-x",
                None,
                Some("rt_1"),
                None
            ),
            Some(0)
        );

        // 2. Match by access token
        assert_eq!(
            find_existing_account_idx_from_parts(
                &accounts,
                "other",
                "other@foo.com",
                "uuid-x",
                None,
                None,
                Some("at_2")
            ),
            Some(1)
        );

        // 3. Match by account_id UUID when email is not conflicting
        assert_eq!(
            find_existing_account_idx_from_parts(&accounts, "", "", "uuid-1", None, None, None),
            Some(0)
        );

        // 4. Match by email case-insensitively when workspace is unassigned
        assert_eq!(
            find_existing_account_idx_from_parts(
                &accounts,
                "",
                "  DEV.USER@EXAMPLE.COM  ",
                "",
                None,
                None,
                None
            ),
            Some(0)
        );

        // 5. Match by ID alias when email is not conflicting
        assert_eq!(
            find_existing_account_idx_from_parts(&accounts, "work", "", "uuid-x", None, None, None),
            Some(1)
        );

        // 6. Same account_id but conflicting email must NOT match
        assert_eq!(
            find_existing_account_idx_from_parts(
                &accounts,
                "other",
                "other@foo.com",
                "uuid-1",
                None,
                None,
                None
            ),
            None
        );

        // 7. Distinct account
        assert_eq!(
            find_existing_account_idx_from_parts(
                &accounts,
                "new_id",
                "new@foo.com",
                "uuid-3",
                None,
                Some("rt_3"),
                Some("at_3")
            ),
            None
        );
    }

    #[test]
    fn test_same_email_different_workspaces_or_plans_never_merge() {
        let accounts = vec![make_test_account(
            "business",
            "dev.user@example.com",
            "uuid-team",
            Some("rt_team"),
            "at_team",
        )];

        // 1. Adding personal account with same email, but different label and different account_id
        let res1 = find_existing_account_idx_from_parts(
            &accounts,
            "dev.user@example.com-[personal]",
            "dev.user@example.com",
            "uuid-personal",
            Some("pro"),
            Some("rt_pro"),
            Some("at_pro"),
        );
        assert_eq!(res1, None);

        // 2. Deduplication check (empty id): different account_id (workspace UUID) prevents merge
        let res2 = find_existing_account_idx_from_parts(
            &accounts,
            "",
            "dev.user@example.com",
            "uuid-personal",
            Some("pro"),
            Some("rt_pro"),
            Some("at_pro"),
        );
        assert_eq!(res2, None);

        // 3. Deduplication check (empty id): different plan prevents merge
        let res3 = find_existing_account_idx_from_parts(
            &accounts,
            "",
            "dev.user@example.com",
            "default",
            Some("pro"),
            Some("rt_pro"),
            Some("at_pro"),
        );
        assert_eq!(res3, None);
    }

    #[test]
    fn test_same_team_workspace_different_emails_never_merge() {
        let mut accounts = vec![
            make_test_account(
                "dev-alt",
                "dev.alt@example.com",
                "3f533057",
                Some("rt_1"),
                "at_1",
            ),
            make_test_account(
                "dev-primary",
                "dev.user@example.com",
                "3f533057",
                Some("rt_2"),
                "at_2",
            ),
        ];

        let _ = deduplicate_accounts(&mut accounts);
        assert_eq!(accounts.len(), 2);
        assert_eq!(accounts[0].email, "dev.alt@example.com");
        assert_eq!(accounts[0].id, "dev.alt@example.com:3f533057");
        assert_eq!(accounts[1].email, "dev.user@example.com");
        assert_eq!(accounts[1].id, "dev.user@example.com:3f533057");
    }

    #[test]
    fn test_deduplicate_accounts_merges_identical_user() {
        let mut accounts = vec![
            make_test_account(
                "main",
                "dev.user@example.com",
                "3f533057",
                Some("rt_same"),
                "at_old",
            ),
            make_test_account(
                "dev-primary",
                "dev.user@example.com",
                "3f533057",
                Some("rt_same"),
                "at_new",
            ),
        ];

        let merged_map = deduplicate_accounts(&mut accounts);
        assert_eq!(accounts.len(), 1);
        assert_eq!(accounts[0].id, "dev.user@example.com:3f533057");
        assert_eq!(accounts[0].name.as_deref(), Some("dev-primary"));
        assert_eq!(accounts[0].tokens.access_token, "at_new");
        assert_eq!(
            merged_map.get("main").map(String::as_str),
            Some("dev.user@example.com:3f533057")
        );
    }

    fn make_test_jwt(email: &str) -> String {
        use base64::engine::general_purpose::URL_SAFE_NO_PAD;
        use base64::Engine;
        let payload = format!(r#"{{"email":"{}"}}"#, email);
        let b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
        format!("eyJhbGciOiJub25lIn0.{}.sig", b64)
    }

    #[test]
    fn test_same_email_and_workspace_different_nicknames_always_merge() {
        let mut file = AccountsFile {
            active_account_id: Some("dev-account".to_string()),
            settings: Default::default(),
            accounts: vec![make_test_account(
                "dev-account",
                "dev@enterprise.example.com",
                "uuid-team",
                Some("rt_1"),
                "at_1",
            )],
        };

        // User tries saving current session under new nickname "dev-account-3"
        let tokens = AuthTokens {
            access_token: "at_updated".to_string(),
            refresh_token: Some("rt_updated".to_string()),
            id_token: Some(make_test_jwt("dev@enterprise.example.com")),
            account_id: Some("uuid-team".to_string()),
        };

        let target_id = add_account_to_accounts_file(&mut file, "dev-account-3", tokens, false);
        // Must merge with existing account, NOT create a second one!
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(target_id, "dev@enterprise.example.com:uuid-team");
        assert_eq!(file.accounts[0].name.as_deref(), Some("dev-account-3"));
    }

    #[test]
    fn test_deduplicate_accounts_file_remaps_active_id() {
        let mut file = AccountsFile {
            active_account_id: Some("main".to_string()),
            settings: Default::default(),
            accounts: vec![
                make_test_account(
                    "main",
                    "dev.user@example.com",
                    "3f533057",
                    Some("rt_same"),
                    "at_old",
                ),
                make_test_account(
                    "dev-primary",
                    "dev.user@example.com",
                    "3f533057",
                    Some("rt_same"),
                    "at_new",
                ),
            ],
        };

        let changed = deduplicate_accounts_file(&mut file);
        assert!(changed);
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(
            file.active_account_id.as_deref(),
            Some("dev.user@example.com:3f533057")
        );
    }

    #[test]
    fn test_add_account_preserves_active_account() {
        let mut file = AccountsFile {
            active_account_id: Some("primary@example.com:uuid-1".to_string()),
            settings: Default::default(),
            accounts: vec![make_test_account(
                "primary@example.com:uuid-1",
                "primary@example.com",
                "uuid-1",
                Some("rt_1"),
                "at_1",
            )],
        };

        let new_tokens = AuthTokens {
            access_token: "at_2".to_string(),
            refresh_token: Some("rt_2".to_string()),
            id_token: None,
            account_id: Some("uuid-2".to_string()),
        };

        let added_id = add_account_to_accounts_file(&mut file, "secondary", new_tokens, true);
        assert_eq!(added_id, "user@openai.com:uuid-2");
        assert_eq!(file.accounts.len(), 2);
        assert_eq!(file.accounts[1].name.as_deref(), Some("secondary"));
        // CRITICAL INVARIANT: active account MUST NOT be replaced!
        assert_eq!(
            file.active_account_id.as_deref(),
            Some("primary@example.com:uuid-1")
        );
    }

    #[test]
    fn test_add_account_sets_active_when_none_existed() {
        let mut file = AccountsFile {
            active_account_id: None,
            settings: Default::default(),
            accounts: vec![],
        };

        let new_tokens = AuthTokens {
            access_token: "at_1".to_string(),
            refresh_token: Some("rt_1".to_string()),
            id_token: None,
            account_id: Some("uuid-1".to_string()),
        };

        let added_id = add_account_to_accounts_file(&mut file, "first", new_tokens, true);
        assert_eq!(added_id, "user@openai.com:uuid-1");
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(
            file.active_account_id.as_deref(),
            Some("user@openai.com:uuid-1")
        );
    }

    #[test]
    fn test_save_current_as_replaces_active_account() {
        let mut file = AccountsFile {
            active_account_id: Some("old@example.com:uuid-1".to_string()),
            settings: Default::default(),
            accounts: vec![make_test_account(
                "old@example.com:uuid-1",
                "old@example.com",
                "uuid-1",
                Some("rt_1"),
                "at_1",
            )],
        };

        let current_tokens = AuthTokens {
            access_token: "at_2".to_string(),
            refresh_token: Some("rt_2".to_string()),
            id_token: None,
            account_id: Some("uuid-2".to_string()),
        };

        let saved_id = add_account_to_accounts_file(&mut file, "new_active", current_tokens, false);
        assert_eq!(saved_id, "user@openai.com:uuid-2");
        assert_eq!(file.accounts.len(), 2);
        assert_eq!(
            file.active_account_id.as_deref(),
            Some("user@openai.com:uuid-2")
        );
    }

    #[test]
    fn test_rename_account_updates_nickname() {
        let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("codex_rename_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_var("CODEX_HOME", &temp_dir);

        let mut file = AccountsFile {
            active_account_id: Some("user1@example.com:uuid-1".to_string()),
            settings: Default::default(),
            accounts: vec![
                make_test_account(
                    "user1@example.com:uuid-1",
                    "user1@example.com",
                    "uuid-1",
                    Some("rt_1"),
                    "at_1",
                ),
                make_test_account(
                    "user2@example.com:uuid-2",
                    "user2@example.com",
                    "uuid-2",
                    Some("rt_2"),
                    "at_2",
                ),
            ],
        };
        file.accounts[0].name = Some("first".to_string());
        file.accounts[1].name = Some("second".to_string());
        crate::storage::save_accounts(&file).unwrap();

        // 1. Rename first account to "work"
        assert!(rename_account("first", Some("work")).is_ok());
        let loaded = crate::storage::load_accounts().unwrap();
        assert_eq!(loaded.accounts[0].name.as_deref(), Some("work"));

        // 2. Renaming to existing nickname "second" must fail with error (avoids duplicate labels!)
        assert!(rename_account("work", Some("second")).is_err());

        // 3. Clear nickname
        assert!(rename_account("work", None).is_ok());
        let loaded = crate::storage::load_accounts().unwrap();
        assert_eq!(loaded.accounts[0].name, None);

        let _ = std::fs::remove_dir_all(&temp_dir);
        std::env::remove_var("CODEX_HOME");
    }

    #[test]
    fn test_apply_relogin_successful_update() {
        let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("codex_relogin_succ_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_var("CODEX_HOME", &temp_dir);

        let mut file = AccountsFile {
            active_account_id: Some("test-user@example.com:uuid-1".to_string()),
            settings: Default::default(),
            accounts: vec![{
                let mut acc = make_test_account(
                    "test-user@example.com:uuid-1",
                    "test-user@example.com",
                    "uuid-1",
                    Some("old_rt"),
                    "old_at",
                );
                acc.name = Some("business".to_string());
                acc.last_error = Some(
                    "401 Unauthorized (Session ended (logged out in app). Re-login required.)"
                        .to_string(),
                );
                acc
            }],
        };

        let jwt = make_test_jwt("test-user@example.com");
        let new_tokens = AuthTokens {
            access_token: jwt.clone(),
            refresh_token: Some("new_rt".to_string()),
            id_token: Some(jwt),
            account_id: Some("uuid-1".to_string()),
        };

        let res = apply_relogin_to_accounts_file(&mut file, "business", new_tokens);
        assert!(res.is_ok());
        assert_eq!(res.unwrap(), "test-user@example.com:uuid-1");
        assert_eq!(file.accounts.len(), 1);
        assert_eq!(file.accounts[0].last_error, None);
        assert_eq!(
            file.accounts[0].tokens.refresh_token.as_deref(),
            Some("new_rt")
        );
        assert!(file.accounts[0].enabled);

        let _ = std::fs::remove_dir_all(&temp_dir);
        std::env::remove_var("CODEX_HOME");
    }

    #[test]
    fn test_apply_relogin_rejects_email_mismatch() {
        let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("codex_relogin_mismatch_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_var("CODEX_HOME", &temp_dir);

        let mut file = AccountsFile {
            active_account_id: Some("test-user@example.com:uuid-1".to_string()),
            settings: Default::default(),
            accounts: vec![{
                let mut acc = make_test_account(
                    "test-user@example.com:uuid-1",
                    "test-user@example.com",
                    "uuid-1",
                    Some("old_rt"),
                    "old_at",
                );
                acc.name = Some("business".to_string());
                acc.last_error = Some("401 Unauthorized".to_string());
                acc
            }],
        };

        // Browser logged into different account "other@example.com"
        let jwt = make_test_jwt("other@example.com");
        let new_tokens = AuthTokens {
            access_token: jwt.clone(),
            refresh_token: Some("other_rt".to_string()),
            id_token: Some(jwt),
            account_id: Some("uuid-2".to_string()),
        };

        let res = apply_relogin_to_accounts_file(&mut file, "business", new_tokens);
        assert!(res.is_err());
        let err_msg = res.unwrap_err();
        assert!(err_msg.contains("Logged in as 'other@example.com'"));
        assert!(err_msg.contains("expected 'test-user@example.com'"));

        // Existing account must remain untouched
        assert_eq!(
            file.accounts[0].tokens.refresh_token.as_deref(),
            Some("old_rt")
        );
        assert_eq!(file.accounts[0].last_error, Some("401 Unauthorized".to_string()));

        let _ = std::fs::remove_dir_all(&temp_dir);
        std::env::remove_var("CODEX_HOME");
    }

    #[test]
    fn test_apply_relogin_syncs_active_auth_json() {
        let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
        let temp_dir =
            std::env::temp_dir().join(format!("codex_relogin_sync_test_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        std::env::set_var("CODEX_HOME", &temp_dir);

        let initial_auth = AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: Some(AuthTokens {
                access_token: "old_active_at".to_string(),
                refresh_token: Some("old_active_rt".to_string()),
                id_token: None,
                account_id: Some("uuid-1".to_string()),
            }),
            last_refresh: None,
        };
        crate::storage::write_active_auth_json(&initial_auth).unwrap();

        let mut file = AccountsFile {
            active_account_id: Some("active@example.com:uuid-1".to_string()),
            settings: Default::default(),
            accounts: vec![make_test_account(
                "active@example.com:uuid-1",
                "active@example.com",
                "uuid-1",
                Some("old_active_rt"),
                "old_active_at",
            )],
        };

        let jwt = make_test_jwt("active@example.com");
        let new_tokens = AuthTokens {
            access_token: jwt.clone(),
            refresh_token: Some("new_active_rt".to_string()),
            id_token: Some(jwt),
            account_id: Some("uuid-1".to_string()),
        };

        let res = apply_relogin_to_accounts_file(&mut file, "active@example.com:uuid-1", new_tokens);
        assert!(res.is_ok());

        let active_auth = crate::storage::read_active_auth_json().unwrap();
        assert_eq!(
            active_auth.tokens.unwrap().refresh_token.as_deref(),
            Some("new_active_rt")
        );

        let _ = std::fs::remove_dir_all(&temp_dir);
        std::env::remove_var("CODEX_HOME");
    }
}
