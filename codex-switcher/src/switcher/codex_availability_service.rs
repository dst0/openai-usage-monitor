use super::{is_codex_app_running, launch_codex_app};

pub(super) struct CodexAvailabilityService;

impl CodexAvailabilityService {
    pub(super) fn relaunch_previous_state(error: String) -> String {
        match launch_codex_app() {
            Ok(_) => format!("{error}; Codex was relaunched with the previous account state"),
            Err(relaunch_error) => {
                format!("{error}; emergency Codex relaunch also failed: {relaunch_error}")
            }
        }
    }

    /// Only an unarchived user task justifies starting a closed Desktop.
    pub(super) fn resume_has_launchable_target(
        targets: &[String],
        is_user_thread: impl Fn(&str) -> bool,
    ) -> bool {
        targets.iter().any(|id| is_user_thread(id))
    }

    /// Resume needs Desktop's same-user IPC owner. When the user asked to
    /// resume work and the standard app is closed, start it instead of failing.
    pub(super) fn ensure_running_for_resume(
        is_running: impl Fn() -> bool,
        launch: impl FnOnce() -> Result<Vec<u32>, String>,
    ) -> Result<(), String> {
        if is_running() {
            return Ok(());
        }
        crate::runtime_print!("CODEX_NOT_RUNNING action=launch reason=resume");
        match launch() {
            Ok(pids) => {
                crate::runtime_print!("CODEX_LAUNCHED pids={pids:?} reason=resume");
                Ok(())
            }
            Err(error) => Err(format!(
                "Codex is not running and could not be started automatically ({error}); start it with `open -a /Applications/ChatGPT.app`, then retry `cxi resume`"
            )),
        }
    }

    pub(super) fn keep_after_failure(error: String) -> String {
        if is_codex_app_running() {
            return error;
        }
        match launch_codex_app() {
            Ok(pids) => format!(
                "{error}; Codex was relaunched after the failed automation with pids={pids:?}"
            ),
            Err(relaunch_error) => {
                format!("{error}; emergency Codex relaunch also failed: {relaunch_error}")
            }
        }
    }
}

#[cfg(test)]
#[path = "codex_availability_service.test.rs"]
mod tests;
