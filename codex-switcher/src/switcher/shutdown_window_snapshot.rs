use crate::distribution::WindowProcessIdentity;

/// What the shutdown window guard saw immediately before a shutdown signal.
pub(super) struct ShutdownWindowSnapshot {
    pub(super) initial_pids: Vec<u32>,
    pub(super) observed: WindowProcessIdentity,
    pub(super) window_ids: Vec<u32>,
    pub(super) current_pids: Vec<u32>,
}
