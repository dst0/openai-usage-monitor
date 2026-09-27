use super::window_restore_process_identity::ProcessIdentity;
use super::window_task_entry::WindowTaskEntry;

/// Every ChatGPT window of one exact Desktop process and the task each
/// showed. It lives only in the restarting process's memory.
#[derive(Clone, Debug, PartialEq)]
pub struct WindowTaskSnapshot {
    pub process: ProcessIdentity,
    pub windows: Vec<WindowTaskEntry>,
}

impl WindowTaskSnapshot {
    /// The WindowServer IDs the shutdown guard may accept, sorted.
    pub fn window_ids(&self) -> Vec<u32> {
        let mut ids: Vec<u32> = self.windows.iter().map(|window| window.window_id).collect();
        ids.sort_unstable();
        ids
    }

    /// The helper's restore plan, sent on its stdin: each window's task and
    /// frame in capture order, and which one should end up focused. Without
    /// recovery tasks it rebuilds the layout after a relaunch; with them it
    /// only moves back a window that recovery moved onto one of those tasks.
    pub fn restore_plan(&self, recovery_task_ids: Option<&[String]>) -> serde_json::Value {
        let windows: Vec<serde_json::Value> = self
            .windows
            .iter()
            .map(|window| {
                serde_json::json!({
                    "task_id": window.task_id,
                    "frame": {
                        "x": window.frame.x,
                        "y": window.frame.y,
                        "width": window.frame.width,
                        "height": window.frame.height,
                    },
                })
            })
            .collect();
        serde_json::json!({
            "windows": windows,
            "focus_index": self.windows.iter().position(|window| window.focused),
            "mode": if recovery_task_ids.is_some() { "recheck" } else { "relaunch" },
            "recovery_task_ids": recovery_task_ids.unwrap_or_default(),
        })
    }
}

#[cfg(test)]
#[path = "window_task_snapshot.test.rs"]
mod tests;
