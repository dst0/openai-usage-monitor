use crate::models::{AccountsFile, AuthJson};
use crate::storage;
use crate::switcher::ActiveAuthRegistrySyncService;

/// Rechecks the decision's inputs after acquiring the shared switch lock.
pub(super) struct DistributionStatePreflightService;

impl DistributionStatePreflightService {
    pub(super) fn verify(
        initial: &AccountsFile,
        expected_cli_id: Option<&str>,
    ) -> Result<(), String> {
        let live = storage::load_accounts()?;
        let initial_value = serde_json::to_value(initial)
            .map_err(|_| "Could not compare distribution account state")?;
        let live_value = serde_json::to_value(&live)
            .map_err(|_| "Could not compare distribution account state")?;
        if initial_value != live_value || live.active_account_id.as_deref() != expected_cli_id {
            return Err("Account state changed before distribution acquired its lock".into());
        }
        let auth = storage::read_active_auth_json()?;
        Self::verify_cli_auth(&live, &auth)
    }

    fn verify_cli_auth(accounts: &AccountsFile, auth: &AuthJson) -> Result<(), String> {
        let active_id = accounts
            .active_account_id
            .as_deref()
            .ok_or("CLI account identity is unavailable")?;
        accounts
            .accounts
            .iter()
            .find(|account| account.id == active_id)
            .ok_or("CLI account identity is unknown")?;
        // Validate full token ownership on a copy: the active auth may have
        // rotated, but no other saved account may own either live token.
        let mut verified = accounts.clone();
        ActiveAuthRegistrySyncService::reconcile(&mut verified, auth)
            .map_err(|_| "CLI authentication does not match a unique saved account")?;
        if verified.active_account_id.as_deref() != Some(active_id) {
            return Err("CLI authentication does not match the active account".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "distribution_state_preflight_service.test.rs"]
mod tests;
