use super::desktop_session_binding_service::DesktopSessionBindingService;
use super::switch_outcome::SwitchOutcome;
use crate::models::{AccountConfig, AccountsFile, AuthJson};

pub(super) struct AccountSwitchNoopService;

impl AccountSwitchNoopService {
    pub(super) fn resolve(
        accounts: &AccountsFile,
        target: &AccountConfig,
        active_auth: Option<&AuthJson>,
        restart_app: bool,
        desktop_running: bool,
    ) -> Result<Option<SwitchOutcome>, String> {
        let is_same_account = accounts
            .active_account_id
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&target.id))
            && active_auth
                .and_then(|auth| auth.tokens.as_ref())
                .is_some_and(|tokens| tokens == &target.tokens);
        if !is_same_account {
            return Ok(None);
        }
        if !desktop_running {
            return Ok(Some(SwitchOutcome {
                recovery_error: None,
            }));
        }
        let home = crate::storage::codex_home();
        if restart_app && !DesktopSessionBindingService::already_bound_to(&home, &target.id) {
            return Ok(None);
        }
        if !restart_app {
            DesktopSessionBindingService::reconcile_current_cli_binding(&home, &target.id)?;
            if !DesktopSessionBindingService::already_bound_to(&home, &target.id) {
                return Err("Running Desktop account binding is unverified".into());
            }
        }
        Ok(Some(SwitchOutcome {
            recovery_error: None,
        }))
    }
}
