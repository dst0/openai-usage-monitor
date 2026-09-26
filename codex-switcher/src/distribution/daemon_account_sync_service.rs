use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens};
use crate::storage::{read_active_auth_json, update_accounts_atomically};

pub struct DaemonAccountSyncService;

impl DaemonAccountSyncService {
    pub fn active_auth_matches_registry(accounts_file: &AccountsFile, auth: &AuthJson) -> bool {
        if auth.auth_mode.as_deref() != Some("chatgpt") {
            return false;
        }
        let Some(auth_tokens) = auth.tokens.as_ref() else {
            return false;
        };
        if auth_tokens.access_token.trim().is_empty() {
            return false;
        }
        let Some(active_id) = accounts_file.active_account_id.as_deref() else {
            return false;
        };
        let Ok(auth_email) = crate::oauth::consistent_jwt_email(auth_tokens) else {
            return false;
        };
        let token_matches = accounts_file
            .accounts
            .iter()
            .filter(|account| tokens_overlap(&account.tokens, auth_tokens))
            .count();
        if token_matches != 1 {
            return false;
        }
        accounts_file
            .accounts
            .iter()
            .find(|account| account.id == active_id)
            .is_some_and(|account| {
                account.tokens == *auth_tokens
                    && identity_matches(
                        account,
                        auth_email.as_deref(),
                        auth_tokens.account_id.as_deref(),
                    )
            })
    }

    pub fn verify_live_active_binding(accounts_file: &AccountsFile) -> Result<(), String> {
        let auth = read_active_auth_json()?;
        if Self::active_auth_matches_registry(accounts_file, &auth) {
            Ok(())
        } else {
            Err("Active Desktop credentials are not bound to one registry account".into())
        }
    }

    pub fn sync_active_tokens_from_auth_obj(
        accounts_file: &mut AccountsFile,
        active_auth: &AuthJson,
    ) -> bool {
        if active_auth.auth_mode.as_deref() != Some("chatgpt") {
            return false;
        }
        let Some(auth_tokens) = active_auth.tokens.as_ref() else {
            return false;
        };
        if auth_tokens.access_token.trim().is_empty() {
            return false;
        }

        let Ok(auth_email) = crate::oauth::consistent_jwt_email(auth_tokens) else {
            return false;
        };
        let (_, auth_plan) = crate::oauth::extract_jwt_metadata_from_tokens(auth_tokens);
        let auth_account_id = auth_tokens.account_id.as_deref();
        let matched_idx = crate::setup::find_existing_account_idx(
            &accounts_file.accounts,
            "",
            auth_email.as_deref().unwrap_or(""),
            auth_account_id.unwrap_or(""),
            auth_plan.as_deref(),
            Some(auth_tokens),
        );

        // A rotated token may still be saved against another account. Never let a
        // token-only match override the Desktop token's own account identity.
        let token_matches: Vec<usize> = accounts_file
            .accounts
            .iter()
            .enumerate()
            .filter_map(|(idx, account)| {
                tokens_overlap(&account.tokens, auth_tokens).then_some(idx)
            })
            .collect();
        if token_matches.len() > 1 {
            return false;
        }

        if let Some(idx) = matched_idx {
            let candidate = &accounts_file.accounts[idx];
            let strong_identity_pair = auth_email.as_deref().is_some_and(real_email)
                && auth_account_id
                    .and_then(real_workspace)
                    .is_some_and(|workspace| account_workspace(candidate) == Some(workspace));
            if (token_matches.is_empty() && !strong_identity_pair)
                || token_matches.first().is_some_and(|matched| *matched != idx)
                || !identity_matches(candidate, auth_email.as_deref(), auth_account_id)
            {
                return false;
            }
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
            return changed;
        }

        if auth_email.as_deref().is_none_or(|email| !real_email(email)) || !token_matches.is_empty()
        {
            return false;
        }

        let added_id = crate::setup::add_account_to_accounts_file(
            accounts_file,
            "",
            auth_tokens.clone(),
            false,
        );
        let added_email = accounts_file
            .accounts
            .iter()
            .find(|account| account.id == added_id)
            .map(|account| account.email.clone())
            .unwrap_or_else(|| auth_email.unwrap_or_else(|| added_id.clone()));
        LogRedactionService::print_background(&format!(
            "✨ Auto-saved newly logged-in account '{}' ({}) from Codex app",
            added_id, added_email
        ));
        crate::switcher::send_macos_notification(
            &format!("New Account Added: {added_id}"),
            &format!("Auto-saved {added_email} from Codex app"),
        );
        true
    }

    pub fn sync_active_tokens(accounts_file: &mut AccountsFile) -> Result<bool, String> {
        Self::sync_active_tokens_with_hook(accounts_file, || Ok(()))
    }

    fn sync_active_tokens_with_hook(
        accounts_file: &mut AccountsFile,
        before_save: impl FnOnce() -> Result<(), String>,
    ) -> Result<bool, String> {
        before_save()?;
        let mut changed = false;
        let fresh = update_accounts_atomically(|registry| {
            // Re-read both registry and active auth under codex.lock. Browser
            // re-login's auth CAS and registry save take the same lock order.
            let active_auth = read_active_auth_json()?;
            changed = Self::sync_active_tokens_from_auth_obj(registry, &active_auth);
            if !Self::active_auth_matches_registry(registry, &active_auth) {
                return Err("Active Desktop credentials could not be uniquely synchronized".into());
            }
            Ok(())
        })?;
        *accounts_file = fresh;
        Ok(changed)
    }

    pub fn cross_pollinate_organization_names(accounts: &mut [AccountConfig]) {
        let mut org_by_workspace = std::collections::HashMap::new();
        for account in accounts.iter() {
            if let Some(organization) = &account.organization_name {
                let workspace = account.account_id.trim();
                if !workspace.is_empty() && workspace != "default" {
                    org_by_workspace.insert(workspace.to_string(), organization.clone());
                }
            }
        }
        for account in accounts.iter_mut() {
            let workspace = account.account_id.trim();
            if account.organization_name.is_none()
                && !workspace.is_empty()
                && workspace != "default"
            {
                if let Some(organization) = org_by_workspace.get(workspace) {
                    account.organization_name = Some(organization.clone());
                }
            }
        }
    }
}

fn real_email(email: &str) -> bool {
    let email = email.trim();
    !email.is_empty()
        && email.contains('@')
        && !email.eq_ignore_ascii_case("user@openai.com")
        && !email.eq_ignore_ascii_case("current-user")
}

fn real_workspace(workspace: &str) -> Option<&str> {
    let workspace = workspace.trim();
    (!workspace.is_empty() && workspace != "default").then_some(workspace)
}

fn account_workspace(account: &AccountConfig) -> Option<&str> {
    account
        .tokens
        .account_id
        .as_deref()
        .and_then(real_workspace)
        .or_else(|| real_workspace(&account.account_id))
}

fn identity_matches(account: &AccountConfig, email: Option<&str>, workspace: Option<&str>) -> bool {
    let email_matches = email.is_none_or(|email| {
        !real_email(email)
            || !real_email(&account.email)
            || account.email.eq_ignore_ascii_case(email)
    });
    let workspace_matches = workspace.is_none_or(|workspace| {
        real_workspace(workspace).is_none_or(|workspace| {
            account_workspace(account).is_none_or(|stored| stored == workspace)
        })
    });
    email_matches && workspace_matches
}

fn tokens_overlap(left: &AuthTokens, right: &AuthTokens) -> bool {
    let same_refresh = right.refresh_token.as_deref().is_some_and(|token| {
        !token.trim().is_empty() && left.refresh_token.as_deref() == Some(token)
    });
    let same_access =
        !right.access_token.trim().is_empty() && left.access_token == right.access_token;
    same_refresh || same_access
}

#[cfg(test)]
#[path = "daemon_account_sync_service.test.rs"]
mod tests;
