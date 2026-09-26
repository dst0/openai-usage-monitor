use super::desktop_session_binding_service::DesktopSessionBindingService;
use super::is_codex_app_running;
use super::switch_outcome::SwitchOutcome;
use crate::models::{AccountConfig, AccountsFile};

pub(super) struct AccountSwitchNoopService;

impl AccountSwitchNoopService {
    pub(super) fn resolve(
        accounts: &AccountsFile,
        target: &AccountConfig,
        restart_app: bool,
    ) -> Result<Option<SwitchOutcome>, String> {
        let active_id = accounts.active_account_id.as_deref();
        let active = accounts
            .accounts
            .iter()
            .find(|account| Some(account.id.as_str()) == active_id);
        let already_active = active_id.is_some_and(|id| id.eq_ignore_ascii_case(&target.id))
            || active.is_some_and(|account| account.id.eq_ignore_ascii_case(&target.id))
            || (target.tokens.refresh_token.is_some()
                && active.and_then(|account| account.tokens.refresh_token.as_ref())
                    == target.tokens.refresh_token.as_ref());
        if !already_active {
            return Ok(None);
        }
        if !restart_app && is_codex_app_running() {
            let recovery_error = DesktopSessionBindingService::reconcile_current_cli_binding(
                &crate::storage::codex_home(),
                &target.id,
            )
            .err();
            return Ok(Some(SwitchOutcome { recovery_error }));
        }
        if !restart_app
            || !is_codex_app_running()
            || DesktopSessionBindingService::already_bound_to(
                &crate::storage::codex_home(),
                &target.id,
            )
        {
            return Ok(Some(SwitchOutcome {
                recovery_error: None,
            }));
        }
        Ok(None)
    }
}
