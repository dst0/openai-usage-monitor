use super::window_restore_backend::WindowRestoreBackend;
use super::window_restore_process_identity::ProcessIdentity;

pub struct WindowProcessValidationService;

impl WindowProcessValidationService {
    pub fn inspect(
        backend: &mut dyn WindowRestoreBackend,
        expected_pid: u32,
    ) -> Result<ProcessIdentity, String> {
        if expected_pid <= 1 {
            return Err("Desktop process PID is invalid".into());
        }
        let observed = backend.inspect_process(expected_pid)?;
        if observed.pid != expected_pid {
            return Err("Window helper returned a different process PID".into());
        }
        ProcessIdentity::new(observed.pid, observed.birth_id)
    }

    pub fn confirm(
        backend: &mut dyn WindowRestoreBackend,
        expected: &ProcessIdentity,
    ) -> Result<(), String> {
        let observed = Self::inspect(backend, expected.pid)?;
        if observed != *expected {
            return Err("Desktop process birth identity changed before shutdown".into());
        }
        Ok(())
    }
}

#[cfg(test)]
#[path = "window_process_validation_service.test.rs"]
mod tests;
