use super::window_restore_frame::WindowFrame;
use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_entry::WindowTaskEntry;
use super::window_task_probe_validation_service::MAX_PROBED_WINDOWS;
use super::window_task_report::WindowTaskReport;
use super::window_task_restore_report::WindowTaskRestoreReport;
use super::window_task_snapshot::WindowTaskSnapshot;
use serde_json::{Map, Value};
use std::collections::HashSet;

const SNAPSHOT_FIELDS: [&str; 3] = ["process", "windows", "clipboard_restored"];
const ENTRY_FIELDS: [&str; 4] = ["window_id", "frame", "task_id", "focused"];
const FRAME_FIELDS: [&str; 4] = ["x", "y", "width", "height"];
const RESTORE_FIELDS: [&str; 3] = ["process", "verified", "clipboard_restored"];
const REHEARSAL_FIELDS: [&str; 4] = [
    "process",
    "window_ids",
    "verified_count",
    "clipboard_restored",
];
const PROCESS_FIELDS: [&str; 2] = ["pid", "birth_id"];
/// The same minimum the native window inventory applies to a user window.
const MIN_WINDOW_WIDTH: f64 = 300.0;
const MIN_WINDOW_HEIGHT: f64 = 250.0;

/// Validates the snapshot, restore, and rehearsal responses of the native
/// helper against exact contracts. Task IDs are accepted only in the snapshot
/// and only in canonical form; no error message carries one.
pub(super) struct WindowTaskSessionValidationService;

impl WindowTaskSessionValidationService {
    pub(super) fn snapshot(
        response: &Value,
        expected: &ProcessIdentity,
    ) -> Result<WindowTaskSnapshot, String> {
        let fields = Self::exact(response, &SNAPSHOT_FIELDS, "Window task snapshot")?;
        Self::process(&fields["process"], expected)?;
        Self::clipboard(&fields["clipboard_restored"])?;
        let windows = fields["windows"]
            .as_array()
            .filter(|windows| !windows.is_empty() && windows.len() <= MAX_PROBED_WINDOWS)
            .ok_or("Window task snapshot has an invalid window count")?;
        let mut ids = HashSet::new();
        let mut tasks = HashSet::new();
        let mut entries = Vec::with_capacity(windows.len());
        for window in windows {
            let entry = Self::entry(window)?;
            if !ids.insert(entry.window_id) {
                return Err("Window task snapshot has a duplicate window ID".into());
            }
            if !tasks.insert(entry.task_id.clone()) {
                return Err("Window task snapshot has a duplicate task".into());
            }
            entries.push(entry);
        }
        if entries.iter().filter(|entry| entry.focused).count() > 1 {
            return Err("Window task snapshot has more than one focused window".into());
        }
        Ok(WindowTaskSnapshot {
            process: expected.clone(),
            windows: entries,
        })
    }

    pub(super) fn restore(
        response: &Value,
        expected: &ProcessIdentity,
        planned: usize,
    ) -> Result<WindowTaskRestoreReport, String> {
        let fields = Self::exact(response, &RESTORE_FIELDS, "Window task restore")?;
        Self::process(&fields["process"], expected)?;
        let clipboard_restored = Self::clipboard(&fields["clipboard_restored"])?;
        let verified: Vec<bool> = fields["verified"]
            .as_array()
            .map(|flags| flags.iter().filter_map(Value::as_bool).collect())
            .unwrap_or_default();
        if verified.len() != planned || fields["verified"].as_array().map(Vec::len) != Some(planned)
        {
            return Err("Window task restore does not match its plan".into());
        }
        Ok(WindowTaskRestoreReport {
            verified,
            clipboard_restored,
        })
    }

    /// A rehearsal succeeds only when every window verified; the helper
    /// reports anything less as a named failure instead.
    pub(super) fn rehearsal(
        response: &Value,
        expected: &ProcessIdentity,
    ) -> Result<WindowTaskReport, String> {
        let fields = Self::exact(response, &REHEARSAL_FIELDS, "Window task rehearsal")?;
        Self::process(&fields["process"], expected)?;
        let clipboard_restored = Self::clipboard(&fields["clipboard_restored"])?;
        let window_ids = Self::window_ids(&fields["window_ids"])?;
        let verified = fields["verified_count"]
            .as_u64()
            .and_then(|count| usize::try_from(count).ok())
            .filter(|count| *count == window_ids.len())
            .ok_or("Window task rehearsal did not verify every window")?;
        Ok(WindowTaskReport {
            windows: window_ids.len(),
            verified,
            clipboard_restored,
        })
    }

    fn exact<'a>(
        value: &'a Value,
        names: &[&str],
        label: &str,
    ) -> Result<&'a Map<String, Value>, String> {
        let fields = value
            .as_object()
            .ok_or_else(|| format!("{label} returned malformed data"))?;
        if fields.len() != names.len() || !names.iter().all(|name| fields.contains_key(*name)) {
            return Err(format!("{label} returned unexpected fields"));
        }
        Ok(fields)
    }

    fn process(value: &Value, expected: &ProcessIdentity) -> Result<(), String> {
        let fields = Self::exact(value, &PROCESS_FIELDS, "Window task helper")?;
        let pid = fields["pid"]
            .as_u64()
            .and_then(|pid| u32::try_from(pid).ok())
            .ok_or("Window task helper omitted the process PID")?;
        let birth = fields["birth_id"]
            .as_str()
            .ok_or("Window task helper omitted the process birth identity")?;
        if ProcessIdentity::new(pid, birth)? != *expected {
            return Err("Window task helper process identity changed".into());
        }
        Ok(())
    }

    fn clipboard(value: &Value) -> Result<bool, String> {
        value
            .as_bool()
            .ok_or_else(|| "Window task helper omitted the clipboard result".into())
    }

    fn window_ids(value: &Value) -> Result<Vec<u32>, String> {
        let ids = value
            .as_array()
            .filter(|ids| !ids.is_empty() && ids.len() <= MAX_PROBED_WINDOWS)
            .ok_or("Window task helper returned an invalid window count")?;
        let mut unique = HashSet::new();
        ids.iter()
            .map(|id| {
                id.as_u64()
                    .and_then(|id| u32::try_from(id).ok())
                    .filter(|id| *id != 0 && unique.insert(*id))
                    .ok_or_else(|| "Window task helper returned an invalid window ID".to_string())
            })
            .collect()
    }

    fn entry(value: &Value) -> Result<WindowTaskEntry, String> {
        let fields = Self::exact(value, &ENTRY_FIELDS, "Window task snapshot")?;
        let window_id = Self::window_ids(&Value::Array(vec![fields["window_id"].clone()]))?[0];
        let frame_fields = Self::exact(&fields["frame"], &FRAME_FIELDS, "Window task snapshot")?;
        let number = |name: &str| {
            frame_fields[name]
                .as_f64()
                .filter(|value| value.is_finite())
        };
        let frame = match (number("x"), number("y"), number("width"), number("height")) {
            (Some(x), Some(y), Some(width), Some(height))
                if width >= MIN_WINDOW_WIDTH && height >= MIN_WINDOW_HEIGHT =>
            {
                WindowFrame {
                    x,
                    y,
                    width,
                    height,
                }
            }
            _ => return Err("Window task snapshot has an invalid window frame".into()),
        };
        let task_id = fields["task_id"]
            .as_str()
            .filter(|task| Self::is_canonical_task_id(task))
            .ok_or("Window task snapshot has an invalid task ID")?
            .to_string();
        let focused = fields["focused"]
            .as_bool()
            .ok_or("Window task snapshot has an invalid focus flag")?;
        Ok(WindowTaskEntry {
            window_id,
            frame,
            task_id,
            focused,
        })
    }

    /// A lowercase hyphenated UUID, the only form ChatGPT copies.
    fn is_canonical_task_id(task: &str) -> bool {
        task.len() == 36
            && task.bytes().enumerate().all(|(index, byte)| match index {
                8 | 13 | 18 | 23 => byte == b'-',
                _ => byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte),
            })
    }
}

#[cfg(test)]
#[path = "window_task_session_validation_service.test.rs"]
mod tests;
