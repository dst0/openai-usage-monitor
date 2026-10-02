use super::desktop_app_session::DesktopAppSession;
use super::distribution_desktop_auth_handoff_service::DistributionDesktopAuthHandoffService;
use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_process_validation_service::WindowProcessValidationService;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_restart_session::WindowTaskRestartSession;
use super::window_task_restore_phase::WindowTaskRestorePhase;
use std::path::PathBuf;
use std::sync::Mutex;

/// One in-memory window-task snapshot for a single distribution transaction.
/// The outer option distinguishes an unstarted capture from verified zero windows.
pub(super) struct DistributionWindowTaskService {
    session: Mutex<Option<Option<WindowTaskRestartSession>>>,
}

impl Default for DistributionWindowTaskService {
    fn default() -> Self {
        Self {
            session: Mutex::new(None),
        }
    }
}

impl DistributionWindowTaskService {
    pub(super) fn capture_with(
        &self,
        expected: &ProcessIdentity,
        backend: &mut SystemWindowRestoreBackend,
        desktop_home: Result<PathBuf, String>,
    ) -> Result<(), String> {
        if self
            .session
            .lock()
            .map_err(|_| "Window task state lock is poisoned")?
            .is_some()
        {
            return Err("A previous window task snapshot is still active".into());
        }
        let inventory = backend.capture_window_inventory(expected.clone())?;
        let session = if inventory.is_empty() {
            None
        } else {
            Some(WindowTaskRestartSession::capture(
                expected,
                desktop_home,
                backend,
                &inventory,
            )?)
        };
        let mut state = self
            .session
            .lock()
            .map_err(|_| "Window task state lock is poisoned")?;
        if state.is_some() {
            return Err("Window task snapshot changed while it was captured".into());
        }
        *state = Some(session);
        Ok(())
    }

    pub(super) fn captured_window_ids(&self) -> Result<Option<Vec<u32>>, String> {
        let state = self
            .session
            .lock()
            .map_err(|_| "Window task state lock is poisoned")?;
        match state.as_ref() {
            Some(Some(session)) => Ok(Some(session.window_ids())),
            Some(None) => Ok(Some(Vec::new())),
            None => Err("Window task capture has not completed".into()),
        }
    }

    pub(super) fn verify_current(&self, expected: &ProcessIdentity) -> Result<(), String> {
        let mut backend = SystemWindowRestoreBackend::new()?;
        self.verify_current_with(expected, &mut backend)
    }

    fn verify_current_with(
        &self,
        expected: &ProcessIdentity,
        backend: &mut SystemWindowRestoreBackend,
    ) -> Result<(), String> {
        let mut state = self
            .session
            .lock()
            .map_err(|_| "Window task state lock is poisoned")?
            .take();
        let result = (|| -> Result<(), String> {
            match state.as_mut() {
                Some(Some(session)) => {
                    let inventory = backend.capture_window_inventory(expected.clone())?;
                    session.verify_unchanged(expected, backend, &inventory)
                }
                Some(None) => {
                    if backend
                        .capture_window_inventory(expected.clone())?
                        .is_empty()
                    {
                        Ok(())
                    } else {
                        Err("Desktop windows appeared after a zero-window capture".into())
                    }
                }
                None => Err("Window task capture has not completed".into()),
            }
        })();
        *self
            .session
            .lock()
            .map_err(|_| "Window task state lock is poisoned")? = state;
        result
    }

    pub(super) fn restore(
        &self,
        process: Result<ProcessIdentity, String>,
        bound: &DesktopAppSession,
        phase: WindowTaskRestorePhase<'_>,
    ) {
        let attempted = self.restore_with(process, phase, SystemWindowRestoreBackend::new, || {
            crate::recovery::wait_for_desktop_ipc()?;
            Self::verify_bound_after_ipc(bound)
        });
        if attempted {
            if let Err(error) = Self::verify_bound_after_ipc(bound) {
                if let Some(Some(session)) = self
                    .session
                    .lock()
                    .unwrap_or_else(|poison| poison.into_inner())
                    .as_mut()
                {
                    session.record_failure(phase, &error);
                }
            }
        }
    }

    fn verify_bound_after_ipc(bound: &DesktopAppSession) -> Result<(), String> {
        DistributionDesktopAuthHandoffService::verify_after_launch(&bound.account_id)?;
        let home = crate::storage::codex_home();
        if DesktopAppSession::load_checked(&home.join("desktop-app-session.json"))?.as_ref()
            != Some(bound)
        {
            return Err("Desktop session changed during window task readiness wait".into());
        }
        let expected = bound
            .process
            .as_ref()
            .ok_or("Desktop session has no process")?;
        if crate::switcher::current_codex_app_pids_checked()? != [expected.pid] {
            return Err("Desktop process set changed during window task readiness wait".into());
        }
        let mut backend = SystemWindowRestoreBackend::new()?;
        if WindowProcessValidationService::inspect(&mut backend, expected.pid)? != *expected {
            return Err("Desktop birth identity changed during window task readiness wait".into());
        }
        Ok(())
    }

    fn restore_with(
        &self,
        process: Result<ProcessIdentity, String>,
        phase: WindowTaskRestorePhase<'_>,
        backend: impl FnOnce() -> Result<SystemWindowRestoreBackend, String>,
        ready: impl FnOnce() -> Result<(), String>,
    ) -> bool {
        let mut state = self
            .session
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .take();
        let mut attempted = false;
        if let Some(Some(session)) = state.as_mut() {
            let needs_restore = match phase {
                WindowTaskRestorePhase::AfterRelaunch => true,
                WindowTaskRestorePhase::AfterRecovery(targets) => !targets.is_empty(),
            };
            if needs_restore {
                attempted = true;
                match process.and_then(|process| backend().map(|backend| (process, backend))) {
                    Ok((process, backend)) => session.restore(&process, &backend, ready, phase),
                    Err(error) => session.record_failure(phase, &error),
                }
            }
        }
        *self.session.lock().unwrap_or_else(|e| e.into_inner()) = state;
        attempted
    }

    pub(super) fn finish(&self) -> Result<(), String> {
        let state = self
            .session
            .lock()
            .map_err(|_| "Window task state lock is poisoned")?
            .take();
        match state {
            Some(Some(session)) => {
                if session.clipboard_kept() {
                    crate::runtime_print!(
                        "WINDOW_TASKS_CLIPBOARD_NOT_RESTORED the clipboard may hold a copied task link"
                    );
                }
                let count = session.window_count();
                session.finish()?;
                crate::runtime_print!("WINDOW_TASKS_RESTORED windows={count}");
                Ok(())
            }
            Some(None) => Ok(()),
            None => Err("Window task capture has not completed".into()),
        }
    }
}

#[cfg(test)]
#[path = "distribution_window_task_service.test.rs"]
mod tests;
