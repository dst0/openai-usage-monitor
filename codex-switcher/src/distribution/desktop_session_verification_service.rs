use super::app_lifecycle::AppLifecycle;
use super::desktop_app_session::DesktopAppSession;
use super::desktop_external_binding_service::DesktopExternalBindingService;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use crate::models::AccountsFile;
use std::path::Path;

/// Verifies that a saved account belongs to the exact live Desktop process.
pub(crate) struct DesktopSessionVerificationService<'a> {
    lifecycle: &'a dyn AppLifecycle,
    home: &'a Path,
}

impl<'a> DesktopSessionVerificationService<'a> {
    pub(crate) fn new(lifecycle: &'a dyn AppLifecycle, home: &'a Path) -> Self {
        Self { lifecycle, home }
    }

    pub(super) fn verified_session(&self) -> Result<DesktopAppSession, String> {
        let session = DesktopAppSession::load_checked(&self.home.join("desktop-app-session.json"))?
            .ok_or("Running Desktop has no process-bound account session")?;
        let process = session
            .process
            .as_ref()
            .ok_or("Running Desktop account session has no process identity")?;
        let observed = self
            .lifecycle
            .inspect_process(process.pid)
            .map_err(|_| "Running Desktop process identity could not be verified")?;
        if observed != *process || !session.matches_process_lifetime(&process.birth_id) {
            return Err("Running Desktop account session does not match the live process".into());
        }
        if let Some(bound_auth_file_id) = session.auth_file_id.as_deref() {
            let current = DesktopExternalBindingService::read_auth_evidence(self.home)
                .ok_or("Running Desktop inferred account auth cannot be verified")?;
            if current.0 != session.account_id || current.1 != bound_auth_file_id {
                return Err("Running Desktop inferred account auth changed".into());
            }
        }
        Ok(session)
    }

    pub(super) fn resolve_current_app_account(
        &self,
        accounts: &AccountsFile,
        request: &DistributionRequest,
    ) -> Result<Option<String>, String> {
        match self.verified_session() {
            Ok(session)
                if accounts
                    .accounts
                    .iter()
                    .any(|account| account.id == session.account_id) =>
            {
                Ok(Some(session.account_id))
            }
            _ if request.trigger.is_user()
                && request.preferred_app_id.is_some()
                && request.allow_restart =>
            {
                // An explicit restart may repair an old or missing binding.
                Ok(None)
            }
            _ => Err("Running Desktop account identity is unverified".into()),
        }
    }

    pub(super) fn verify_plan_before_mutation(
        &self,
        plan: &DistributionPlan,
        request: &DistributionRequest,
    ) -> Result<(), String> {
        let running = self.lifecycle.is_app_running()?;
        if plan.current_app_id.is_some() && !running {
            return Err("Desktop process changed before distribution could start".into());
        }
        if plan.current_app_id.is_none() && !plan.restart_required && running {
            return Err("Desktop process changed before distribution could start".into());
        }
        if !running {
            return Ok(());
        }
        if plan.app_switch_needed
            && !plan.restart_required
            && plan.target_app_id != plan.current_app_id
        {
            return Err("Changing a running Desktop account requires a restart".into());
        }
        if plan.current_app_id.is_none() {
            if plan.restart_required
                && request.trigger.is_user()
                && request.preferred_app_id.is_some()
                && request.allow_restart
            {
                return Ok(());
            }
            return Err("Running Desktop account identity is unverified".into());
        }
        let session = self.verified_session()?;
        if plan.current_app_id.as_deref() != Some(session.account_id.as_str()) {
            return Err("Desktop account changed before distribution could start".into());
        }
        Ok(())
    }

    pub(super) fn verify_no_restart_target(
        &self,
        accounts: &AccountsFile,
        current_app_id: Option<&str>,
        request: &DistributionRequest,
    ) -> Result<(), String> {
        if request.allow_restart {
            return Ok(());
        }
        let Some(preferred_app_id) = request.preferred_app_id.as_deref() else {
            return Ok(());
        };
        let idx = crate::switcher::resolve_target_account_idx(&accounts.accounts, preferred_app_id)
            .map_err(|_| "Requested Desktop account is unknown")?;
        if current_app_id != Some(accounts.accounts[idx].id.as_str()) {
            return Err("Changing a running Desktop account requires a restart".into());
        }
        Ok(())
    }

    pub(crate) fn reconcile_cli_binding(&self, cli_account_id: &str) -> Result<(), String> {
        let session = self.verified_session()?;
        if session.account_id != cli_account_id {
            return Err("Desktop and CLI account bindings differ".into());
        }
        self.save_cli_binding(session, cli_account_id)
    }

    fn save_cli_binding(
        &self,
        session: DesktopAppSession,
        cli_account_id: &str,
    ) -> Result<(), String> {
        let process = session
            .process
            .ok_or("Desktop process identity is missing")?;
        let mut updated =
            DesktopAppSession::bound(session.account_id, cli_account_id, process.clone());
        updated.auth_file_id = session.auth_file_id;
        updated.save(&self.home.join("desktop-app-session.json"))?;
        if self.lifecycle.inspect_process(process.pid)? != process {
            return Err("Desktop process changed after CLI binding".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "desktop_session_verification_service.test.rs"]
mod tests;
