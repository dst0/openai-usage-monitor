use super::window_restore_process_identity::ProcessIdentity;
use std::collections::HashSet;

/// Same limit the native helper enforces before it focuses any window.
const MAX_PROBED_WINDOWS: usize = 64;
/// The complete response contract. Task IDs never leave the native helper, so
/// any additional field is a protocol violation rather than extra detail.
const RESPONSE_FIELDS: [&str; 3] = ["process", "window_ids", "observed_task_count"];
/// Fixed failure codes the probe path of the native helper may print. They
/// carry no task, window, or clipboard data, so they are shown verbatim; any
/// other stderr is replaced by the generic helper error. `COMMAND_REJECTED`
/// means the installed helper predates the probe.
const FAILURE_CODES: [&str; 16] = [
    "COMMAND_REJECTED",
    "EXPLICIT_OPT_IN_REQUIRED",
    "PROBE_ACCESS_DENIED",
    "PROCESS_IDENTITY_REJECTED",
    "WINDOW_NOT_FOUND",
    "WINDOW_ACCESS_FAILED",
    "WINDOW_GEOMETRY_FAILED",
    "WINDOW_INVENTORY_MISMATCH",
    "WINDOW_LIMIT_EXCEEDED",
    "WINDOW_MINIMIZED",
    "WINDOW_MAPPING_AMBIGUOUS",
    "WINDOW_FOCUS_FAILED",
    "COPY_LINK_EVENT_FAILED",
    "COPY_LINK_AMBIGUOUS",
    "TASK_LINK_DUPLICATE",
    "WINDOW_MAPPING_CHANGED",
];

/// Validates the native diagnostic response without storing or logging task IDs.
pub(super) struct WindowTaskProbeValidationService;

impl WindowTaskProbeValidationService {
    pub(super) fn parse(
        response: &serde_json::Value,
        expected: &ProcessIdentity,
    ) -> Result<usize, String> {
        let fields = response
            .as_object()
            .ok_or("Task probe returned malformed data")?;
        if fields.len() != RESPONSE_FIELDS.len()
            || !RESPONSE_FIELDS
                .iter()
                .all(|field| fields.contains_key(*field))
        {
            return Err("Task probe returned unexpected fields".into());
        }
        let pid = response["process"]["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or("Task probe omitted process PID")?;
        let birth = response["process"]["birth_id"]
            .as_str()
            .ok_or("Task probe omitted process birth identity")?;
        if ProcessIdentity::new(pid, birth)? != *expected {
            return Err("Task probe process identity changed".into());
        }
        let ids = response["window_ids"]
            .as_array()
            .ok_or("Task probe omitted window IDs")?;
        let observed = response["observed_task_count"]
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .ok_or("Task probe omitted selected-task count")?;
        if ids.is_empty() || ids.len() > MAX_PROBED_WINDOWS || ids.len() != observed {
            return Err("Task probe window count is ambiguous".into());
        }
        let mut window_ids = HashSet::new();
        for id in ids {
            let id = id
                .as_u64()
                .and_then(|id| u32::try_from(id).ok())
                .filter(|id| *id != 0)
                .ok_or("Task probe has invalid window ID")?;
            if !window_ids.insert(id) {
                return Err("Task probe has duplicate window ID".into());
            }
        }
        Ok(ids.len())
    }

    /// Names a known probe failure; `None` leaves the generic helper error.
    pub(super) fn failure(stderr: &[u8]) -> Option<String> {
        let code = stderr.strip_suffix(b"\n").unwrap_or(stderr);
        FAILURE_CODES
            .iter()
            .find(|known| known.as_bytes() == code)
            .map(|known| format!("Task probe failed: {known}"))
    }
}

#[cfg(test)]
#[path = "window_task_probe_validation_service.test.rs"]
mod tests;
