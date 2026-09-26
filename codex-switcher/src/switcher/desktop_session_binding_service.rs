use crate::distribution::{
    desktop_session_verification_service::DesktopSessionVerificationService, AppLifecycle,
    DesktopAppSession, SystemAppLifecycle, SystemWindowRestoreBackend, WindowProcessIdentity,
    WindowProcessValidationService,
};
use crate::models::{AccountsFile, AuthJson};
use std::path::Path;

/// Keeps direct `cxi switch` Desktop restarts aligned with distribution.
pub(super) struct DesktopSessionBindingService;

impl DesktopSessionBindingService {
    pub(super) fn reconcile_current_cli_binding(
        home: &Path,
        account_id: &str,
    ) -> Result<(), String> {
        let lifecycle = SystemAppLifecycle::default();
        Self::reconcile_with(home, account_id, Self::verified_cli_account_id, &lifecycle)
    }

    fn reconcile_with(
        home: &Path,
        account_id: &str,
        verified_cli: impl FnOnce() -> Result<String, String>,
        lifecycle: &dyn AppLifecycle,
    ) -> Result<(), String> {
        if verified_cli()? != account_id {
            return Err("Active CLI authentication does not match the requested account".into());
        }
        DesktopSessionVerificationService::new(lifecycle, home).reconcile_cli_binding(account_id)
    }

    pub(super) fn verified_cli_account_id() -> Result<String, String> {
        let accounts = crate::storage::load_accounts()?;
        let auth = crate::storage::read_active_auth_json()?;
        Self::resolve_cli_account_id(&accounts, &auth)
    }

    fn resolve_cli_account_id(accounts: &AccountsFile, auth: &AuthJson) -> Result<String, String> {
        let active_id = accounts
            .active_account_id
            .as_deref()
            .ok_or("CLI account identity is unavailable")?;
        let account = accounts
            .accounts
            .iter()
            .find(|account| account.id == active_id)
            .ok_or("CLI account identity is unknown")?;
        if auth
            .tokens
            .as_ref()
            .and_then(|tokens| tokens.account_id.as_deref())
            != Some(account.account_id.as_str())
        {
            return Err("CLI authentication does not match the active account".into());
        }
        Ok(account.id.clone())
    }

    pub(super) fn already_bound_to(home: &Path, account_id: &str) -> bool {
        let pids = super::current_codex_app_pids();
        if pids.len() != 1 {
            return false;
        }
        let Some(session) = DesktopAppSession::load(&home.join("desktop-app-session.json")) else {
            return false;
        };
        if session.account_id != account_id || session.cli_account_id.as_deref() != Some(account_id)
        {
            return false;
        }
        let Some(process) = session.process.as_ref() else {
            return false;
        };
        if process.pid != pids[0] || !session.matches_process_lifetime(&process.birth_id) {
            return false;
        }
        matches!(Self::inspect_current(pids[0]), Ok(observed) if observed == *process)
            && super::current_codex_app_pids() == pids
    }

    pub(super) fn bind_then_recover(
        home: &Path,
        account_id: &str,
        pid: u32,
        recover: impl FnOnce(&WindowProcessIdentity) -> Result<(), String>,
    ) -> Result<(), String> {
        Self::bind_then_recover_with(
            home,
            account_id,
            pid,
            super::current_codex_app_pids,
            Self::inspect_current,
            recover,
        )
    }

    pub(super) fn bind_current_cli_after_emergency_launch(
        home: &Path,
        pids: &[u32],
    ) -> Result<(), String> {
        if pids.len() != 1 {
            return Err("Emergency Desktop launch did not produce one process".into());
        }
        let account_id = Self::verified_cli_account_id()?;
        Self::bind_then_recover(home, &account_id, pids[0], |_| Ok(()))
    }

    pub(super) fn confirm_after_recovery(expected: &WindowProcessIdentity) -> Result<(), String> {
        if super::current_codex_app_pids() != [expected.pid]
            || Self::inspect_current(expected.pid)? != *expected
        {
            return Err("Desktop process identity changed during account recovery".into());
        }
        Ok(())
    }

    fn inspect_current(pid: u32) -> Result<WindowProcessIdentity, String> {
        let mut backend = SystemWindowRestoreBackend::new()?;
        WindowProcessValidationService::inspect(&mut backend, pid)
    }

    fn bind_with(
        home: &Path,
        account_id: &str,
        pid: u32,
        mut pids: impl FnMut() -> Vec<u32>,
        mut inspect: impl FnMut(u32) -> Result<WindowProcessIdentity, String>,
    ) -> Result<WindowProcessIdentity, String> {
        if pids() != [pid] {
            return Err("Desktop process set changed before session binding".into());
        }
        let before = inspect(pid)?;
        let after = inspect(pid)?;
        if before != after || pids() != [pid] {
            return Err("Desktop process identity changed before session binding".into());
        }
        DesktopAppSession::bound(account_id, account_id, after.clone())
            .save(&home.join("desktop-app-session.json"))?;
        Ok(after)
    }

    fn bind_then_recover_with(
        home: &Path,
        account_id: &str,
        pid: u32,
        pids: impl FnMut() -> Vec<u32>,
        inspect: impl FnMut(u32) -> Result<WindowProcessIdentity, String>,
        recover: impl FnOnce(&WindowProcessIdentity) -> Result<(), String>,
    ) -> Result<(), String> {
        let bound = Self::bind_with(home, account_id, pid, pids, inspect)?;
        recover(&bound)
    }
}

#[cfg(test)]
#[path = "desktop_session_binding_service.test.rs"]
mod tests;
