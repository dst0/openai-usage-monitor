use crate::models::{AccountConfig, AuthJson};
use crate::storage::{read_active_auth_json, update_accounts_atomically};

pub(super) struct AccountSwitchCommitService;

impl AccountSwitchCommitService {
    pub(super) fn commit(
        target: &AccountConfig,
        expected_previous_id: Option<&str>,
    ) -> Result<(), String> {
        Self::commit_with_hook(target, expected_previous_id, || Ok(()))
    }

    fn commit_with_hook(
        target: &AccountConfig,
        expected_previous_id: Option<&str>,
        before_commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        before_commit()?;
        update_accounts_atomically(|latest| {
            let active_id = latest.active_account_id.as_deref();
            if active_id != expected_previous_id && active_id != Some(target.id.as_str()) {
                return Err("Active account changed before switch commit".into());
            }
            let mut matches = latest
                .accounts
                .iter()
                .filter(|account| account.id.eq_ignore_ascii_case(&target.id));
            let current = matches
                .next()
                .ok_or("Selected account disappeared before switch commit")?;
            if matches.next().is_some() {
                return Err("Selected account became ambiguous before switch commit".into());
            }
            if current.tokens != target.tokens {
                return Err("Selected account credentials changed before switch commit".into());
            }
            if current.email != target.email
                || current.account_id != target.account_id
                || current.enabled != target.enabled
                || current.needs_relogin()
            {
                return Err(
                    "Selected account identity or eligibility changed before switch commit".into(),
                );
            }
            latest.active_account_id = Some(current.id.clone());
            Ok(())
        })?;
        Ok(())
    }

    pub(super) fn verify_auth_after_commit(committed: &AuthJson) -> Result<(), String> {
        Self::verify_auth_after_commit_with(committed, || Ok(()))
    }

    fn verify_auth_after_commit_with(
        committed: &AuthJson,
        before_readback: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        before_readback()?;
        let observed = read_active_auth_json().map_err(|_| {
            "Shared authentication could not be read after switch commit".to_string()
        })?;
        if observed != *committed {
            return Err(
                "Shared authentication changed after switch commit; Desktop remains stopped".into(),
            );
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "account_switch_commit_service.test.rs"]
mod tests;
