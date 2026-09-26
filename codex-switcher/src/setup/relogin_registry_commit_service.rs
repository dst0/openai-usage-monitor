use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{read_active_auth_json, update_accounts_atomically};

/// Commits one re-logged account without replacing concurrent registry state.
pub(super) struct ReloginRegistryCommitService<'a> {
    original: &'a AccountConfig,
    updated_id: &'a str,
}

impl<'a> ReloginRegistryCommitService<'a> {
    pub(super) fn new(original: &'a AccountConfig, updated_id: &'a str) -> Self {
        Self {
            original,
            updated_id,
        }
    }

    pub(super) fn commit(
        &self,
        staged: &AccountsFile,
        expected_active_auth: Option<&AuthJson>,
    ) -> Result<(), String> {
        let updated = staged
            .accounts
            .iter()
            .find(|account| account.id == self.updated_id)
            .ok_or("Re-login target disappeared before registry commit")?;
        update_accounts_atomically(|fresh| {
            match expected_active_auth {
                Some(expected) => {
                    if read_active_auth_json()? != *expected {
                        return Err("Active credentials changed before registry commit".into());
                    }
                    if fresh.active_account_id.as_deref() != Some(self.original.id.as_str()) {
                        return Err("Active account changed before registry commit".into());
                    }
                }
                None if fresh.active_account_id.as_deref() == Some(self.original.id.as_str()) => {
                    return Err("Re-login target became active before registry commit".into());
                }
                None => {}
            }
            let index = fresh
                .accounts
                .iter()
                .position(|account| account.id == self.original.id)
                .ok_or("Re-login target changed before registry commit")?;
            let current = &fresh.accounts[index];
            if !current
                .email
                .trim()
                .eq_ignore_ascii_case(self.original.email.trim())
                || current.account_id != self.original.account_id
                || current.tokens.account_id != self.original.tokens.account_id
                || (expected_active_auth.is_none() && current.tokens != self.original.tokens)
                || (updated.id != self.original.id
                    && fresh
                        .accounts
                        .iter()
                        .any(|account| account.id == updated.id))
            {
                return Err("Re-login target identity changed before registry commit".into());
            }
            let current = &mut fresh.accounts[index];
            current.id = updated.id.clone();
            current.email = updated.email.clone();
            current.account_id = updated.account_id.clone();
            current.plan_type = updated.plan_type.clone();
            current.tokens = updated.tokens.clone();
            current.enabled = updated.enabled;
            current.last_error = updated.last_error.clone();
            if expected_active_auth.is_some() {
                fresh.active_account_id = Some(updated.id.clone());
            }
            Ok(())
        })?;
        Ok(())
    }
}
