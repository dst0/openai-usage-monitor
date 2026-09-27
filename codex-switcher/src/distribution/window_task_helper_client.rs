use super::system_window_restore_backend::SystemWindowRestoreBackend;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_command::WindowTaskCommand;
use super::window_task_probe_validation_service::WindowTaskProbeValidationService;
use std::io::{Read, Write};
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

const HELPER_REJECTED: &str = "Codex window restore helper rejected the request";
/// The largest restore plan the helper reads from stdin.
const MAX_INPUT_BYTES: usize = 256 * 1024;
const POLL_INTERVAL: Duration = Duration::from_millis(50);
/// After the helper's group is killed its pipes close; readers get this
/// long to drain before the call returns without them.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Runs one window-task command of the native helper for an exact process.
/// Input, such as a restore plan with task IDs, goes through stdin, never
/// argv. The helper runs in its own process group and is killed with every
/// descendant when its deadline passes, so a pasteboard prompt nobody answers
/// or an unresponsive Accessibility server cannot hold a restart forever.
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
        timeout: Duration,
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
            .process_group(0)
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
            let input = input.to_vec();
            // A helper that exits without reading is reported by its status.
            thread::spawn(move || {
                let _ = stdin.write_all(&input);
            });
        }
        let stdout = Self::drain(child.stdout.take());
        let stderr = Self::drain(child.stderr.take());
        let status = Self::wait(&mut child, timeout);
        let stdout = stdout.recv_timeout(DRAIN_TIMEOUT).unwrap_or_default();
        let stderr = stderr.recv_timeout(DRAIN_TIMEOUT).unwrap_or_default();
        let status = match status {
            Ok(Some(status)) => status,
            Ok(None) => {
                return Err(format!(
                    "{} timed out after {}s; the helper was stopped",
                    command.label(),
                    timeout.as_secs()
                ))
            }
            Err(()) => {
                return Err(format!(
                    "{} could not be waited for; the helper was stopped",
                    command.label()
                ))
            }
        };
        if !status.success() {
            return Err(
                WindowTaskProbeValidationService::failure(&stderr, command.label())
                    .unwrap_or_else(|| HELPER_REJECTED.into()),
            );
        }
        serde_json::from_slice(&stdout)
            .map_err(|_| "Codex window restore helper returned invalid data".into())
    }

    /// The exit status; `Ok(None)` after killing the helper's whole process
    /// group once `timeout` passes, `Err` when its status cannot be read.
    fn wait(child: &mut Child, timeout: Duration) -> Result<Option<std::process::ExitStatus>, ()> {
        let deadline = Instant::now() + timeout;
        let outcome = loop {
            match child.try_wait() {
                Ok(Some(status)) => return Ok(Some(status)),
                Ok(None) if Instant::now() < deadline => thread::sleep(POLL_INTERVAL),
                // One last look, so an exit right at the deadline counts.
                Ok(None) => match child.try_wait() {
                    Ok(Some(status)) => return Ok(Some(status)),
                    _ => break Ok(None),
                },
                Err(_) => break Err(()),
            }
        };
        if let Ok(group) = libc::pid_t::try_from(child.id()) {
            // SAFETY: signals only the group this call created for the helper.
            unsafe { libc::kill(-group, libc::SIGKILL) };
        }
        let _ = child.kill();
        let _ = child.wait();
        outcome
    }

    fn drain(pipe: Option<impl Read + Send + 'static>) -> mpsc::Receiver<Vec<u8>> {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || {
            let mut bytes = Vec::new();
            if let Some(mut pipe) = pipe {
                let _ = pipe.read_to_end(&mut bytes);
            }
            let _ = sender.send(bytes);
        });
        receiver
    }
}

#[cfg(test)]
#[path = "window_task_helper_client.test.rs"]
mod tests;
