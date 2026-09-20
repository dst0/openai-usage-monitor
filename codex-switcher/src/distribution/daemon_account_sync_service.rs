use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{read_active_auth_json, save_accounts};

pub struct DaemonAccountSyncService;

impl DaemonAccountSyncService {
    pub fn sync_active_tokens_from_auth_obj(
        accounts_file: &mut AccountsFile,
        active_auth: &AuthJson,
    ) -> bool {
        let Some(auth_tokens) = active_auth.tokens.as_ref() else {
            return false;
        };
        if auth_tokens.access_token.trim().is_empty() {
            return false;
        }

        let (auth_email, auth_plan) = crate::oauth::extract_jwt_metadata_from_tokens(auth_tokens);
        let auth_account_id = auth_tokens.account_id.as_deref();
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
            return changed;
        }

        let valid_email = auth_email
            .as_ref()
            .map(|email| {
                let email = email.trim();
                !email.is_empty()
                    && email.contains('@')
                    && !email.eq_ignore_ascii_case("user@openai.com")
                    && !email.eq_ignore_ascii_case("current-user")
            })
            .unwrap_or(false);
        if !valid_email {
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
        let active_auth = match read_active_auth_json() {
            Ok(auth) => auth,
            Err(_) => return Ok(false),
        };
        let changed = Self::sync_active_tokens_from_auth_obj(accounts_file, &active_auth);
        if changed {
            save_accounts(accounts_file)?;
        }
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

#[cfg(test)]
#[path = "daemon_account_sync_service.test.rs"]
mod tests;
