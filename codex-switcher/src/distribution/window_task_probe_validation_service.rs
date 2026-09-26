use super::window_restore_process_identity::ProcessIdentity;
use std::collections::HashSet;

/// Validates the native diagnostic response without storing or logging task IDs.
pub(super) struct WindowTaskProbeValidationService;

impl WindowTaskProbeValidationService {
    pub(super) fn parse(
        response: &serde_json::Value,
        expected: &ProcessIdentity,
    ) -> Result<usize, String> {
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
        if ids.is_empty() || ids.len() > 64 || ids.len() != observed {
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
}
