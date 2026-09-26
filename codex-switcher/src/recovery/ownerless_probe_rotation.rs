use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

/// `CODEX_HOME` cursors the production rotation keeps between prune passes.
pub(super) const MAX_ROTATION_HOMES: usize = 128;

/// Round-robin cursors that pick which ownerless rollout may use the scan
/// budget of one prune pass, so an older long rollout cannot starve the other
/// cold tasks. Each `CODEX_HOME` has its own cursor, so probing another home
/// cannot skip this home's turn. The prune service owns the rotation:
/// production shares one process-wide instance, while tests inject isolated
/// instances that parallel tests cannot advance or evict.
pub(super) struct OwnerlessProbeRotation {
    capacity: usize,
    /// Ordered from the least to the most recently selected home.
    cursors: Mutex<Vec<(PathBuf, usize)>>,
}

impl OwnerlessProbeRotation {
    pub(super) const fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "a rotation must hold at least one home");
        Self {
            capacity,
            cursors: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn shared() -> &'static Self {
        static SHARED: OwnerlessProbeRotation = OwnerlessProbeRotation::new(MAX_ROTATION_HOMES);
        &SHARED
    }

    /// Index of the ownerless target to scan in this pass over `home`. A pass
    /// without ownerless targets leaves every cursor unchanged, so a
    /// detection-only pass cannot skip a deferred target's turn. A full
    /// rotation forgets the least recently selected home, never the home that
    /// is taking its turn.
    pub(super) fn select(&self, home: &Path, ownerless_count: usize) -> Option<usize> {
        if ownerless_count == 0 {
            return None;
        }
        let mut cursors = self
            .cursors
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let cursor = match cursors.iter().position(|(cached, _)| cached == home) {
            Some(index) => cursors.remove(index).1,
            None => {
                if cursors.len() >= self.capacity {
                    cursors.remove(0);
                }
                0
            }
        };
        let selected = cursor % ownerless_count;
        cursors.push((home.to_path_buf(), (selected + 1) % ownerless_count));
        Some(selected)
    }
}
