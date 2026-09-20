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
