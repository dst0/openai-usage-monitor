use crate::models::{AccountsFile, AuthJson};
use crate::storage::{self, update_accounts_atomically};

pub(crate) struct ActiveAuthRegistrySyncService;

impl ActiveAuthRegistrySyncService {
    /// Persist Desktop's latest rotated credentials before replacing auth.json.
    /// A missing auth file is allowed only for an initial CLI setup.
    pub(crate) fn sync_from_disk(accounts: &mut AccountsFile) -> Result<Option<AuthJson>, String> {
        Self::sync_from_disk_with_hook(accounts, || Ok(()))
    }

    fn sync_from_disk_with_hook(
        accounts: &mut AccountsFile,
        before_save: impl FnOnce() -> Result<(), String>,
    ) -> Result<Option<AuthJson>, String> {
        if !storage::auth_json_path().exists() {
            return Ok(None);
        }
        before_save()?;
        let mut observed = None;
        let fresh = update_accounts_atomically(|registry| {
            let auth = storage::read_active_auth_json()?;
            Self::reconcile(registry, &auth)?;
            observed = Some(auth);
            Ok(())
        })?;
        *accounts = fresh;
        Ok(observed)
    }

    pub(crate) fn reconcile(accounts: &mut AccountsFile, auth: &AuthJson) -> Result<bool, String> {
        if auth.auth_mode.as_deref() != Some("chatgpt") {
            return Err("Active Desktop authentication mode is unsupported".into());
        }
        let tokens = auth
            .tokens
            .as_ref()
            .filter(|tokens| !tokens.access_token.trim().is_empty())
            .ok_or("Active Desktop authentication is unavailable")?;
        let provider = tokens
            .account_id
            .as_deref()
            .filter(|id| !id.trim().is_empty() && *id != "default")
            .ok_or("Active Desktop account identity is unavailable")?;
        let email = crate::oauth::consistent_jwt_email(tokens)?;
        let email = email.as_deref();
        let mut matching = accounts.accounts.iter().enumerate().filter(|(_, account)| {
            account.account_id == provider
                && account.tokens.account_id.as_deref() == Some(provider)
                && match email {
                    Some(email) => account.email.eq_ignore_ascii_case(email),
                    None => account.tokens == *tokens,
                }
        });
        let index = matching
            .next()
            .map(|(index, _)| index)
            .ok_or("Active Desktop account has no unique saved identity")?;
        if matching.next().is_some() {
            return Err("Active Desktop account identity is ambiguous".into());
        }
        let account = &mut accounts.accounts[index];
        let mut changed = false;
        if account.tokens != *tokens {
            account.tokens = tokens.clone();
            account.last_error = None;
            changed = true;
        }
        if accounts.active_account_id.as_deref() != Some(account.id.as_str()) {
            accounts.active_account_id = Some(account.id.clone());
            changed = true;
        }
        Ok(changed)
    }
}

#[cfg(test)]
#[path = "active_auth_registry_sync_service.test.rs"]
mod tests;
