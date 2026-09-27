use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_command::WindowTaskCommand;
use super::window_task_probe_validation_service::WindowTaskProbeValidationService;
use std::io::Write;
use std::path::Path;
use std::process::{Command, Stdio};

const HELPER_REJECTED: &str = "Codex window restore helper rejected the request";
/// The largest restore plan the helper reads from stdin.
const MAX_INPUT_BYTES: usize = 256 * 1024;

/// Runs one window-task command of the native helper for an exact process.
/// Input, such as a restore plan with task IDs, goes through stdin, never
/// argv, so it does not appear in the process table.
pub(super) struct WindowTaskHelperClient<'a> {
    helper: &'a Path,
}

impl<'a> WindowTaskHelperClient<'a> {
    pub(super) fn new(backend: &'a SystemWindowRestoreBackend) -> Self {
        Self {
            helper: backend.helper_path(),
        }
    }

    pub(super) fn run(
        &self,
        command: WindowTaskCommand,
        process: &ProcessIdentity,
        input: Option<&[u8]>,
    ) -> Result<serde_json::Value, String> {
        if input.is_some_and(|input| input.len() > MAX_INPUT_BYTES) {
            return Err(format!("{} plan is too large", command.label()));
        }
        let mut child = Command::new(self.helper)
            .args([
                command.helper_command(),
                "--expected-pid",
                &process.pid.to_string(),
                "--expected-birth",
                &process.birth_id,
                "--allow-focus-and-clipboard",
                "yes",
            ])
            .stdin(if input.is_some() {
                Stdio::piped()
            } else {
                Stdio::null()
            })
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|_| "Codex window restore helper could not start".to_string())?;
        if let (Some(input), Some(mut stdin)) = (input, child.stdin.take()) {
            // A helper that exits without reading is reported by its status.
            let _ = stdin.write_all(input);
        }
        let output = child
            .wait_with_output()
            .map_err(|_| "Codex window restore helper could not finish".to_string())?;
        if !output.status.success() {
            return Err(
                WindowTaskProbeValidationService::failure(&output.stderr, command.label())
                    .unwrap_or_else(|| HELPER_REJECTED.into()),
            );
        }
        serde_json::from_slice(&output.stdout)
            .map_err(|_| "Codex window restore helper returned invalid data".into())
    }
}

#[cfg(test)]
#[path = "window_task_helper_client.test.rs"]
mod tests;
