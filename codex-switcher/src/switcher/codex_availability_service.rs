use super::desktop_session_binding_service::DesktopSessionBindingService;
use super::{is_shared_auth_active_checked, launch_codex_app};
use crate::models::AuthJson;
use crate::storage::{
    compare_and_remove_active_auth_json, compare_and_write_active_auth_json_for_switch,
    read_active_auth_json, write_active_auth_json_if_absent,
};

pub(super) struct CodexAvailabilityService;

impl CodexAvailabilityService {
    pub(super) fn replace_auth_for_switch(
        previous: Option<&AuthJson>,
        replacement: &AuthJson,
    ) -> Result<(), String> {
        Self::replace_auth_for_switch_with(previous, replacement, is_shared_auth_active_checked)
    }

    fn replace_auth_for_switch_with(
        previous: Option<&AuthJson>,
        replacement: &AuthJson,
        shared_auth_active: impl FnMut() -> Result<bool, String>,
    ) -> Result<(), String> {
        match previous {
            Some(expected) => compare_and_write_active_auth_json_for_switch(
                expected,
                replacement,
                shared_auth_active,
            ),
            None => write_active_auth_json_if_absent(replacement, shared_auth_active),
        }
    }

    pub(super) fn restore_auth_without_relaunch(
        previous: Option<&AuthJson>,
        committed: &AuthJson,
        error: String,
    ) -> String {
        Self::restore_auth_without_relaunch_with(
            previous,
            committed,
            error,
            is_shared_auth_active_checked,
        )
    }

    fn restore_auth_without_relaunch_with(
        previous: Option<&AuthJson>,
        committed: &AuthJson,
        error: String,
        mut shared_auth_active: impl FnMut() -> Result<bool, String>,
    ) -> String {
        let restore = match previous {
            Some(previous) => compare_and_write_active_auth_json_for_switch(
                committed,
                previous,
                &mut shared_auth_active,
            ),
            None => compare_and_remove_active_auth_json(committed, &mut shared_auth_active),
        };
        if let Err(restore) = restore {
            return format!("{error}; previous authentication restoration failed: {restore}");
        }
        let verified = match previous {
            Some(previous) => read_active_auth_json().is_ok_and(|active| active == *previous),
            None => std::fs::symlink_metadata(crate::storage::auth_json_path())
                .is_err_and(|error| error.kind() == std::io::ErrorKind::NotFound),
        };
        if !verified {
            return format!("{error}; previous authentication readback failed");
        }
        error
    }

    #[cfg(test)]
    fn restore_auth_then_relaunch_with(
        previous: &AuthJson,
        committed: &AuthJson,
        error: String,
        is_running: impl FnOnce() -> Result<bool, String>,
        restore: impl FnOnce(&AuthJson, &AuthJson) -> Result<(), String>,
        readback: impl FnOnce() -> Result<AuthJson, String>,
        launch: impl FnOnce() -> Result<Vec<u32>, String>,
    ) -> String {
        match is_running() {
            Ok(false) => {}
            Ok(true) => {
                return format!("{error}; Desktop started before authentication could be restored")
            }
            Err(inspection) => {
                return format!("{error}; Desktop process inspection failed: {inspection}")
            }
        }
        if let Err(restore_error) = restore(committed, previous) {
            return format!(
                "{error}; previous authentication restoration failed: {restore_error}; Desktop remains stopped"
            );
        }
        let verified = readback().is_ok_and(|active| active == *previous);
        if !verified {
            return format!(
                "{error}; previous authentication readback failed; Desktop remains stopped"
            );
        }
        match launch() {
            Ok(pids) if pids.len() == 1 => {
                format!("{error}; Desktop relaunched with the previous account")
            }
            Ok(pids) => format!(
                "{error}; previous Desktop relaunch produced {} main processes",
                pids.len()
            ),
            Err(relaunch_error) => {
                format!("{error}; previous Desktop relaunch failed: {relaunch_error}")
            }
        }
    }

    pub(super) fn relaunch_previous_state(error: String) -> String {
        match is_shared_auth_active_checked() {
            Ok(true) => return format!("{error}; Desktop is already running"),
            Err(inspection) => {
                return format!("{error}; Desktop process inspection failed: {inspection}")
            }
            Ok(false) => {}
        }
        match launch_codex_app() {
            Ok(pids) => {
                match DesktopSessionBindingService::bind_current_cli_after_emergency_launch(
                    &crate::storage::codex_home(),
                    &pids,
                ) {
                    Ok(()) => {
                        format!("{error}; Codex was relaunched with the previous account state")
                    }
                    Err(binding) => {
                        format!("{error}; Codex relaunched but account binding failed: {binding}")
                    }
                }
            }
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
        match is_shared_auth_active_checked() {
            Ok(true) => return error,
            Err(inspection) => {
                return format!("{error}; Desktop process inspection failed: {inspection}")
            }
            Ok(false) => {}
        }
        match launch_codex_app() {
            Ok(pids) => {
                match DesktopSessionBindingService::bind_current_cli_after_emergency_launch(
                    &crate::storage::codex_home(),
                    &pids,
                ) {
                    Ok(()) => format!("{error}; Codex was relaunched after the failed automation"),
                    Err(binding) => {
                        format!("{error}; Codex relaunched but account binding failed: {binding}")
                    }
                }
            }
            Err(relaunch_error) => {
                format!("{error}; emergency Codex relaunch also failed: {relaunch_error}")
            }
        }
    }
}

#[cfg(test)]
#[path = "codex_availability_service.test.rs"]
mod tests;
