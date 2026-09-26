use crate::{
    distribution::{DesktopAppSession, SystemWindowRestoreBackend, WindowProcessValidationService},
    storage, switcher,
};

pub(super) struct DesktopAccountBindingService;

impl DesktopAccountBindingService {
    /// The CLI auth file may have been deliberately rotated after Desktop was
    /// launched. Accept only the managed Desktop session for this exact live
    /// ChatGPT process; an absent or stale session cannot authorize dispatch.
    pub(super) fn verified(cli_account_id: Option<&str>) -> Option<String> {
        let mut backend = SystemWindowRestoreBackend::new().ok()?;
        let accounts = storage::load_accounts().ok()?;
        verified_with(
            cli_account_id,
            switcher::current_codex_app_pids,
            |pid| WindowProcessValidationService::inspect(&mut backend, pid).ok(),
            || DesktopAppSession::load(&storage::codex_home().join("desktop-app-session.json")),
            |id| accounts.accounts.iter().any(|account| account.id == id),
        )
    }
}

fn verified_with(
    cli_account_id: Option<&str>,
    mut pids: impl FnMut() -> Vec<u32>,
    mut inspect: impl FnMut(u32) -> Option<crate::distribution::WindowProcessIdentity>,
    session: impl FnOnce() -> Option<DesktopAppSession>,
    known_account: impl FnOnce(&str) -> bool,
) -> Option<String> {
    let before_pids = pids();
    if before_pids.len() != 1 {
        return None;
    }
    let before = inspect(before_pids[0])?;
    let session = session()?;
    if !known_account(&session.account_id)
        || !session_matches_binding(&session, &before, cli_account_id)
        || !session.matches_process_lifetime(&before.birth_id)
    {
        return None;
    }
    let after = inspect(before_pids[0])?;
    if before != after || pids() != before_pids {
        return None;
    }
    Some(session.account_id)
}

fn session_matches_binding(
    session: &DesktopAppSession,
    process: &crate::distribution::WindowProcessIdentity,
    cli_account_id: Option<&str>,
) -> bool {
    cli_account_id.is_some()
        && session.cli_account_id.as_deref() == cli_account_id
        && session.process.as_ref() == Some(process)
}

pub(super) fn choose_recovery_account_binding(
    cli: Option<&str>,
    desktop: Option<&str>,
    deferred: bool,
) -> Option<String> {
    if deferred { desktop } else { cli }.map(str::to_owned)
}

#[cfg(test)]
#[path = "desktop_account_binding_service.test.rs"]
mod tests;
