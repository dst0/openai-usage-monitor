use crate::models::{AccountConfig, AuthJson};

/// Refuses a direct switch before any journal, credential, or Desktop change.
pub(super) struct AccountSwitchPreflightService;

impl AccountSwitchPreflightService {
    /// Returns whether ChatGPT Desktop is running. Production probes read the
    /// live process table; a probe failure blocks the switch.
    pub(super) fn check(
        target: &AccountConfig,
        auth_before_stop: Option<&AuthJson>,
        desktop_running: impl FnOnce() -> Result<bool, String>,
        shared_auth_active: impl FnOnce() -> Result<bool, String>,
    ) -> Result<bool, String> {
        let desktop_running = desktop_running()?;
        if shared_auth_active()? && !desktop_running {
            return Err(
                "A bundled Desktop credential writer is running without its main process".into(),
            );
        }
        if target.needs_relogin() {
            let relogin_hint = target.name.as_deref().unwrap_or(&target.id);
            return Err(format!(
                "Account '{}' ({}) requires re-login before switching. Please run 'cxi relogin \"{}\"' first.",
                target.display_name(),
                target.email,
                relogin_hint
            ));
        }
        if desktop_running && auth_before_stop.is_none() {
            return Err("Running Desktop has no readable authentication".into());
        }
        Ok(desktop_running)
    }
}

#[cfg(test)]
#[path = "account_switch_preflight_service.test.rs"]
mod tests;
