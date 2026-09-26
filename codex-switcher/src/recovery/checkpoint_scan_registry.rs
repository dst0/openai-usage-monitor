use super::{
    checkpoint_confirmation::CheckpointConfirmation, checkpoint_scan_cache::CachedCheckpointScan,
    pending_target::PendingTarget,
};
use crate::switcher;
use std::{
    fs::Metadata,
    os::unix::fs::MetadataExt,
    path::{Path, PathBuf},
    sync::Mutex,
};

/// Checkpoints the production registry keeps between deferred probes, per
/// cache.
pub(super) const MAX_CACHED_CHECKPOINTS: usize = 128;
type CacheKey = (PathBuf, u64);
/// Entries ordered from the least to the most recently probed.
type ProbeOrder<T> = Mutex<Vec<(CacheKey, T)>>;

/// Bounded caches of checkpoint scans, keyed by rollout path and checkpoint
/// offset: the append cursors of deferred probes and the pinned-snapshot
/// confirmations that destructive journal changes require. A full cache
/// evicts its least recently probed entry, usually a target that already
/// left the journal, and only after the new probe succeeded; a probe whose
/// scan fails or whose file changed drops its own entry instead. An append
/// cursor that saw a malformed or oversized record is kept on purpose: it
/// reports no evidence, and rescanning would find the same record. The
/// callers own the registry: production shares one process-wide instance,
/// while tests use isolated instances that parallel tests cannot evict.
pub(super) struct CheckpointScanRegistry {
    capacity: usize,
    scans: ProbeOrder<CachedCheckpointScan>,
    confirmations: ProbeOrder<CheckpointConfirmation>,
}

impl CheckpointScanRegistry {
    pub(super) const fn new(capacity: usize) -> Self {
        assert!(capacity > 0, "a scan registry must hold at least one entry");
        Self {
            capacity,
            scans: Mutex::new(Vec::new()),
            confirmations: Mutex::new(Vec::new()),
        }
    }

    pub(super) fn shared() -> &'static Self {
        static SHARED: CheckpointScanRegistry = CheckpointScanRegistry::new(MAX_CACHED_CHECKPOINTS);
        &SHARED
    }

    /// Continues the cached append cursor after `target`'s checkpoint within
    /// `budget`. Returns `(started, verified)` only once the scan has reached
    /// the rollout length seen at the start of the call.
    pub(super) fn post_checkpoint_status_with_budget(
        &self,
        home: &Path,
        target: &PendingTarget,
        budget: &mut u64,
    ) -> Option<(bool, bool)> {
        let (key, metadata) = checkpoint_key(home, target)?;
        let path = key.0.clone();
        let mut scans = self.scans.lock().unwrap_or_else(|error| error.into_inner());
        let cached = take(&mut scans, &key).filter(|scan| scan.can_continue(&metadata));
        let mut scan = match cached {
            Some(scan) => scan,
            None => CachedCheckpointScan::new(path.clone(), key.1, &metadata)?,
        };
        // Bound work while the global recovery operation lock is held.
        // Incomplete evidence cannot authorize an IPC dispatch; the next probe
        // resumes here.
        let start = scan.observer.offset;
        let limit = metadata.len().min(start.saturating_add(*budget));
        *budget -= limit - start;
        let status = scan.scan_to(limit).ok();
        let after = path.metadata().ok();
        let identity_stable = after.as_ref().is_some_and(|after| {
            after.dev() == metadata.dev()
                && after.ino() == metadata.ino()
                && after.len() >= metadata.len()
                && (after.len() > metadata.len()
                    || (after.modified().ok() == metadata.modified().ok()
                        && (after.ctime(), after.ctime_nsec())
                            == (metadata.ctime(), metadata.ctime_nsec())))
        });
        if !identity_stable || status.is_none() {
            return None;
        }
        scan.update_identity(&metadata);
        let result = if scan.observer.saw_oversized || scan.observer.saw_malformed {
            None
        } else if limit == metadata.len()
            && after.is_some_and(|after| after.len() == metadata.len())
        {
            status
        } else {
            None
        };
        keep_most_recent(&mut scans, self.capacity, key, scan);
        result
    }

    /// Destructive journal changes require lifecycle proof from a fresh scan
    /// of one unchanged rollout snapshot. Cached append evidence only starts
    /// this confirmation; it never authorizes checkpoint replacement or
    /// pruning.
    pub(super) fn confirmed_checkpoint_status_with_budget(
        &self,
        home: &Path,
        target: &PendingTarget,
        budget: &mut u64,
    ) -> Option<(bool, bool)> {
        let (key, metadata) = checkpoint_key(home, target)?;
        let path = key.0.clone();
        let mut confirmations = self
            .confirmations
            .lock()
            .unwrap_or_else(|error| error.into_inner());
        let cached = take(&mut confirmations, &key).filter(|scan| scan.matches(&metadata));
        let mut scan = match cached {
            Some(scan) => scan,
            None => CheckpointConfirmation::new(path.clone(), key.1, &metadata)?,
        };
        // A failed scan or a changed file discards this confirmation without
        // evicting another one.
        let status = scan.scan_with_budget(budget).ok()?;
        if !path
            .metadata()
            .ok()
            .is_some_and(|after| scan.matches(&after))
        {
            return None;
        }
        keep_most_recent(&mut confirmations, self.capacity, key, scan);
        status
    }

    /// Reads a completed confirmation without counting it as a probe. A
    /// poisoned cache fails closed.
    pub(super) fn confirmed_snapshot_still_current(
        &self,
        home: &Path,
        target: &PendingTarget,
    ) -> bool {
        let Some((key, metadata)) = checkpoint_key(home, target) else {
            return false;
        };
        self.confirmations.lock().ok().is_some_and(|confirmations| {
            confirmations
                .iter()
                .find(|(cached, _)| *cached == key)
                .is_some_and(|(_, scan)| scan.is_complete() && scan.matches(&metadata))
        })
    }

    #[cfg(test)]
    pub(super) fn scanned_bytes_for(&self, path: &Path, offset: u64) -> Option<u64> {
        inspect(&self.scans, path, offset, |scan| scan.scanned_bytes)
    }

    #[cfg(test)]
    pub(super) fn cached_partial_capacity_for(&self, path: &Path, offset: u64) -> Option<usize> {
        inspect(&self.scans, path, offset, |scan| {
            scan.observer.partial.capacity()
        })
    }

    #[cfg(test)]
    pub(super) fn confirmation_cursor_for(&self, path: &Path, offset: u64) -> Option<u64> {
        inspect(
            &self.confirmations,
            path,
            offset,
            CheckpointConfirmation::cursor,
        )
    }
}

/// The cache key for `target` and the rollout metadata observed now, or `None`
/// when the checkpoint cannot be scanned.
fn checkpoint_key(home: &Path, target: &PendingTarget) -> Option<(CacheKey, Metadata)> {
    let offset = target.offset?;
    let path = switcher::find_thread_rollout_path(home, &target.id)?;
    let metadata = path.metadata().ok()?;
    (metadata.len() >= offset).then_some(((path, offset), metadata))
}

/// Removes an entry for a probe; only a probe that succeeds puts it back.
fn take<T>(entries: &mut Vec<(CacheKey, T)>, key: &CacheKey) -> Option<T> {
    let index = entries.iter().position(|(cached, _)| cached == key)?;
    Some(entries.remove(index).1)
}

fn keep_most_recent<T>(entries: &mut Vec<(CacheKey, T)>, capacity: usize, key: CacheKey, value: T) {
    if entries.len() >= capacity {
        entries.remove(0);
    }
    entries.push((key, value));
}

#[cfg(test)]
fn inspect<T, R>(
    entries: &ProbeOrder<T>,
    path: &Path,
    offset: u64,
    read: impl FnOnce(&T) -> R,
) -> Option<R> {
    entries
        .lock()
        .unwrap_or_else(|error| error.into_inner())
        .iter()
        .find(|(key, _)| key.0 == path && key.1 == offset)
        .map(|(_, entry)| read(entry))
}
