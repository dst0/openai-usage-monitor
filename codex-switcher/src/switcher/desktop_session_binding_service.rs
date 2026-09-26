use crate::distribution::{
    DesktopAppSession, SystemWindowRestoreBackend, WindowProcessIdentity,
    WindowProcessValidationService,
};
use std::path::Path;

pub(super) struct DesktopSessionBindingService;

impl DesktopSessionBindingService {
    pub(super) fn bind_launched(
        home: &Path,
        account_id: &str,
        pid: u32,
    ) -> Result<WindowProcessIdentity, String> {
        Self::bind_with(
            home,
            account_id,
            pid,
            super::current_codex_app_pids,
            |pid| {
                let mut backend = SystemWindowRestoreBackend::new()?;
                WindowProcessValidationService::inspect(&mut backend, pid)
            },
        )
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
}

#[cfg(test)]
#[path = "desktop_session_binding_service.test.rs"]
mod tests;
