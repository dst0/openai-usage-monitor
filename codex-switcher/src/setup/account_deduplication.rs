use crate::models::{AccountConfig, AccountsFile};
use std::collections::HashMap;

use super::{build_predictable_account_id, find_existing_account_idx};

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
