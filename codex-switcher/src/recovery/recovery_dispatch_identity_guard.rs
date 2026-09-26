use super::{
    desktop_account_binding_service::DesktopAccountBindingService,
    manifest_store::current_account_binding, recovery_banner::RecoveryBanner,
    recovery_mode::RecoveryMode,
};
use crate::{
    distribution::{
        SystemWindowRestoreBackend, WindowProcessIdentity, WindowProcessValidationService,
    },
    switcher,
};

pub(super) struct RecoveryDispatchIdentityGuard {
    account_id: String,
    process: WindowProcessIdentity,
}

impl RecoveryDispatchIdentityGuard {
    pub(super) fn capture(banner: &RecoveryBanner, mode: RecoveryMode) -> Result<Self, String> {
        Self::capture_with(
            banner,
            mode,
            current_account_binding,
            Self::live_process,
            |account| DesktopAccountBindingService::verified_process(Some(account)),
        )
    }

    fn capture_with(
        banner: &RecoveryBanner,
        mode: RecoveryMode,
        mut account: impl FnMut() -> Option<String>,
        mut live_process: impl FnMut() -> Result<WindowProcessIdentity, String>,
        mut verified_desktop_process: impl FnMut(&str) -> Option<WindowProcessIdentity>,
    ) -> Result<Self, String> {
        let account_id = account().ok_or("Could not verify the active account before recovery")?;
        let hidden_relaunch = !banner.has_visible_panel() && mode == RecoveryMode::CapturedRestart;
        let process = if banner.has_visible_panel() {
            banner.panel_process()?
        } else if hidden_relaunch {
            // A hidden pre-shutdown banner still names the old process. Only
            // captured relaunches with an exact live Desktop session marker
            // may bind the new singleton before owner discovery. Other modes
            // must keep the original process identity through the wait.
            let bound = verified_desktop_process(&account_id)
                .ok_or("Could not verify relaunched Desktop for hidden recovery")?;
            if live_process()? != bound {
                return Err("Relaunched Desktop process changed after session verification".into());
            }
            bound
        } else {
            banner.expected_process().clone()
        };
        let guard = Self {
            account_id,
            process,
        };
        guard
            .verify_with(&mut account, &mut live_process)
            .map_err(|error| error.to_string())?;
        if hidden_relaunch
            && verified_desktop_process(&guard.account_id).as_ref() != Some(&guard.process)
        {
            return Err("Relaunched Desktop session changed during recovery capture".into());
        }
        Ok(guard)
    }

    pub(super) fn account_id(&self) -> &str {
        &self.account_id
    }

    pub(super) fn verify(&self) -> Result<(), super::dispatch_mark_error::DispatchMarkError> {
        self.verify_with(current_account_binding, Self::live_process)
    }

    fn verify_with(
        &self,
        mut account: impl FnMut() -> Option<String>,
        mut process: impl FnMut() -> Result<WindowProcessIdentity, String>,
    ) -> Result<(), super::dispatch_mark_error::DispatchMarkError> {
        use super::dispatch_mark_error::DispatchMarkError;
        if account().as_deref() != Some(self.account_id.as_str()) {
            return Err(DispatchMarkError::AccountChanged);
        }
        if process().ok().as_ref() != Some(&self.process) {
            return Err(DispatchMarkError::AccountChanged);
        }
        if account().as_deref() != Some(self.account_id.as_str()) {
            return Err(DispatchMarkError::AccountChanged);
        }
        if process().ok().as_ref() != Some(&self.process) {
            return Err(DispatchMarkError::AccountChanged);
        }
        Ok(())
    }

    fn live_process() -> Result<WindowProcessIdentity, String> {
        let pids = switcher::current_codex_app_pids();
        if pids.len() != 1 {
            return Err("Recovery requires one exact ChatGPT main process".into());
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        WindowProcessValidationService::inspect(&mut backend, pids[0])
    }
}

#[cfg(test)]
#[path = "recovery_dispatch_identity_guard.test.rs"]
mod tests;
