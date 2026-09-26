use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage;

/// Commits only the selected account marker against a fresh registry.
pub(super) struct DistributionOfflineRegistryService;

impl DistributionOfflineRegistryService {
    pub(super) fn commit(
        target: &AccountConfig,
        expected_previous_id: Option<&str>,
    ) -> Result<AccountsFile, String> {
        storage::update_accounts_atomically(|fresh| {
            if fresh.active_account_id.as_deref() != expected_previous_id {
                return Err("Active account changed before offline distribution commit".into());
            }
            let mut matches = fresh
                .accounts
                .iter()
                .filter(|account| account.id.eq_ignore_ascii_case(&target.id));
            let latest = matches
                .next()
                .ok_or("Target account disappeared before offline distribution commit")?;
            if matches.next().is_some() {
                return Err(
                    "Target account became ambiguous before offline distribution commit".into(),
                );
            }
            if latest.tokens != target.tokens
                || latest.email != target.email
                || latest.account_id != target.account_id
                || !latest.enabled
                || latest.needs_relogin()
            {
                return Err("Target account changed before offline distribution commit".into());
            }
            fresh.active_account_id = Some(latest.id.clone());
            Ok(())
        })
    }

    pub(super) fn verify_rollback_binding(
        expected_target_id: &str,
        before: &AccountsFile,
        previous_auth: &AuthJson,
    ) -> Result<(), String> {
        let latest = storage::load_accounts()?;
        Self::validate_previous_binding(&latest, before, previous_auth, Some(expected_target_id))
    }

    pub(super) fn rollback(
        expected_target_id: &str,
        before: &AccountsFile,
        previous_auth: &AuthJson,
    ) -> Result<AccountsFile, String> {
        storage::update_accounts_atomically(|fresh| {
            Self::validate_previous_binding(
                fresh,
                before,
                previous_auth,
                Some(expected_target_id),
            )?;
            fresh.active_account_id = before.active_account_id.clone();
            Ok(())
        })
    }

    pub(super) fn verify_restored(
        before: &AccountsFile,
        previous_auth: &AuthJson,
    ) -> Result<(), String> {
        let latest = storage::load_accounts()?;
        Self::validate_previous_binding(
            &latest,
            before,
            previous_auth,
            before.active_account_id.as_deref(),
        )
    }

    fn validate_previous_binding(
        latest: &AccountsFile,
        before: &AccountsFile,
        previous_auth: &AuthJson,
        expected_active: Option<&str>,
    ) -> Result<(), String> {
        if latest.active_account_id.as_deref() != expected_active {
            return Err("Active account changed before offline distribution rollback".into());
        }
        let Some(previous_id) = before.active_account_id.as_deref() else {
            return if previous_auth.tokens.is_none() {
                Ok(())
            } else {
                Err("Previous account binding is unavailable for rollback".into())
            };
        };
        let mut saved = before
            .accounts
            .iter()
            .filter(|account| account.id == previous_id);
        let baseline = saved
            .next()
            .ok_or("Previous account disappeared from rollback snapshot")?;
        if saved.next().is_some() {
            return Err("Previous account was ambiguous in rollback snapshot".into());
        }
        let mut matching = latest
            .accounts
            .iter()
            .filter(|account| account.id == previous_id);
        let current = matching
            .next()
            .ok_or("Previous account disappeared before rollback")?;
        if matching.next().is_some()
            || current.tokens != baseline.tokens
            || previous_auth.tokens.as_ref() != Some(&current.tokens)
            || current.email != baseline.email
            || current.account_id != baseline.account_id
        {
            return Err("Previous account credentials changed before rollback".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "distribution_offline_registry_service.test.rs"]
mod tests;
