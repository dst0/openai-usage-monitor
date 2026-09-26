use super::ipc_call_error::IpcCallError;

pub(super) fn open_then_wait_for_owner<T>(
    open: impl FnOnce() -> Result<(), String>,
    wait_for_owner: impl FnOnce() -> Result<T, IpcCallError>,
) -> Result<T, IpcCallError> {
    if let Err(error) = open() {
        if crate::switcher::is_fatal_thread_navigation_error(&error) {
            return Err(IpcCallError::Other(error));
        }
        crate::logger::log(
            "WARN",
            "RECOVERY",
            "INITIAL_TASK_NAVIGATION_FAILED; owner discovery continues",
        );
    }
    wait_for_owner()
}

#[cfg(test)]
#[path = "ownerless_link_mount.test.rs"]
mod tests;
