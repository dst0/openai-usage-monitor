use super::checkpoint_scan_cache::CachedCheckpointScan;
use std::{fs::Metadata, os::unix::fs::MetadataExt, path::PathBuf, time::SystemTime};

/// A second scan from the saved checkpoint. Unlike the append cursor used by
/// deferred probes, this scan is pinned to one exact rollout snapshot. It can
/// span bounded locked passes, but any append or rewrite starts it over.
pub(super) struct CheckpointConfirmation {
    scan: CachedCheckpointScan,
    length: u64,
    device: u64,
    inode: u64,
    modified: Option<SystemTime>,
    changed_at: (i64, i64),
    complete: Option<(bool, bool)>,
}

impl CheckpointConfirmation {
    pub(super) fn new(path: PathBuf, offset: u64, metadata: &Metadata) -> Option<Self> {
        Some(Self {
            scan: CachedCheckpointScan::new(path, offset, metadata)?,
            length: metadata.len(),
            device: metadata.dev(),
            inode: metadata.ino(),
            modified: metadata.modified().ok(),
            changed_at: (metadata.ctime(), metadata.ctime_nsec()),
            complete: None,
        })
    }

    pub(super) fn matches(&self, metadata: &Metadata) -> bool {
        metadata.dev() == self.device
            && metadata.ino() == self.inode
            && metadata.len() == self.length
            && metadata.modified().ok() == self.modified
            && (metadata.ctime(), metadata.ctime_nsec()) == self.changed_at
    }

    /// `Ok(None)` means the pinned snapshot is not fully proven yet; `Err`
    /// means the scan failed and this confirmation must be discarded.
    pub(super) fn scan_with_budget(
        &mut self,
        budget: &mut u64,
    ) -> Result<Option<(bool, bool)>, String> {
        if let Some(status) = self.complete {
            return Ok(Some(status));
        }
        let start = self.scan.observer.offset;
        let limit = self.length.min(start.saturating_add(*budget));
        *budget -= limit - start;
        let status = self.scan.scan_to(limit)?;
        // A writer can have emitted useful work while a later task_complete
        // error is still an incomplete JSONL record. Such a snapshot cannot
        // authorize removal or replacement of the saved retry.
        if limit == self.length
            && self.scan.observer.offset == self.length
            && !self.scan.observer.oversized
            && !self.scan.observer.saw_oversized
            && !self.scan.observer.saw_malformed
        {
            self.complete = Some(status);
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }

    pub(super) fn is_complete(&self) -> bool {
        self.complete.is_some()
    }

    #[cfg(test)]
    pub(super) fn cursor(&self) -> u64 {
        self.scan.observer.offset
    }
}
