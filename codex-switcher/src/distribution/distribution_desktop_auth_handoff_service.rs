use crate::models::AccountsFile;
use crate::storage;
use crate::switcher::ActiveAuthRegistrySyncService;

/// Preserves a Desktop refresh that occurred during shutdown, before auth.json
/// is replaced with the next account's credentials.
pub(super) struct DistributionDesktopAuthHandoffService;

impl DistributionDesktopAuthHandoffService {
    /// Reject a stale Desktop marker or registry snapshot before signalling
    /// the live process. A token refresh for the same uniquely identified
    /// account is acceptable and will be saved after shutdown.
    pub(super) fn verify_before_stop(
        accounts: &AccountsFile,
        expected_app_id: &str,
        expected_cli_id: &str,
    ) -> Result<(), String> {
        let active = storage::read_active_auth_json()?;
        let mut matched = accounts.clone();
        ActiveAuthRegistrySyncService::reconcile(&mut matched, &active)?;
        if matched.active_account_id.as_deref() != Some(expected_app_id)
            || matched.active_account_id.as_deref() != Some(expected_cli_id)
        {
            return Err("Live Desktop authentication differs from the planned account".into());
        }
        Ok(())
    }

    /// Desktop may replace the shared auth file during its own launch. Check
    /// the exact target again immediately before owner-routed recovery.
    pub(super) fn verify_after_launch(expected_account_id: &str) -> Result<(), String> {
        let first = storage::read_active_auth_json()?;
        let mut accounts = storage::load_accounts()?;
        ActiveAuthRegistrySyncService::reconcile(&mut accounts, &first)?;
        if accounts.active_account_id.as_deref() != Some(expected_account_id) {
            return Err("Relaunched Desktop authentication differs from recovery target".into());
        }
        if storage::read_active_auth_json()? != first {
            return Err("Relaunched Desktop authentication changed during recovery check".into());
        }
        Ok(())
    }

    pub(super) fn preserve_after_stop(
        accounts: &mut AccountsFile,
        expected_current_app_id: &str,
    ) -> Result<(), String> {
        Self::preserve_after_stop_with_hook(accounts, expected_current_app_id, || Ok(()))
    }

    fn preserve_after_stop_with_hook(
        accounts: &mut AccountsFile,
        expected_current_app_id: &str,
        before_registry_commit: impl FnOnce() -> Result<(), String>,
    ) -> Result<(), String> {
        before_registry_commit()?;
        let latest = storage::update_accounts_atomically(|fresh| {
            let active = storage::read_active_auth_json()?;
            ActiveAuthRegistrySyncService::reconcile(fresh, &active)?;
            if !fresh
                .active_account_id
                .as_deref()
                .is_some_and(|id| id.eq_ignore_ascii_case(expected_current_app_id))
            {
                return Err("Desktop authentication identity changed during shutdown".into());
            }
            Ok(())
        })?;
        *accounts = latest;
        Ok(())
    }
}

#[cfg(test)]
#[path = "distribution_desktop_auth_handoff_service.test.rs"]
mod tests;
