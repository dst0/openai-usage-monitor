use crate::storage;

pub(super) struct ActiveAuthBindingService;

impl ActiveAuthBindingService {
    /// During a Desktop restart the auth file is committed before the CLI
    /// registry is updated. Resolve the actual auth identity uniquely instead
    /// of relying on an active-account pointer that can lag the transaction.
    pub(super) fn current() -> Option<String> {
        let mut accounts = storage::load_accounts().ok()?;
        let auth = storage::read_active_auth_json().ok()?;
        // Reconciliation validates provider, email and token ownership. This
        // copy resolves a rotated Desktop token without changing the registry.
        crate::switcher::ActiveAuthRegistrySyncService::reconcile(&mut accounts, &auth).ok()?;
        accounts.active_account_id
    }
}

#[cfg(test)]
#[path = "active_auth_binding_service.test.rs"]
mod tests;
