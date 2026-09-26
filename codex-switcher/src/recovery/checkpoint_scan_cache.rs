use super::observer::Observer;
use std::{
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::SystemTime,
};

const BOUNDARY_BYTES: u64 = 64;
const MAX_CACHED_TURN_ID_BYTES: usize = 128;

/// An append-only rollout cursor. It retains parsed lifecycle evidence but
/// discards incomplete line contents, so a cold task cannot pin a large
/// buffer in the daemon for the duration of the quota window.
pub(super) struct CachedCheckpointScan {
    pub(super) observer: Observer,
    device: u64,
    inode: u64,
    checkpoint_offset: u64,
    checkpoint_prefix: Vec<u8>,
    observed_length: u64,
    modified: Option<SystemTime>,
    boundary: Vec<u8>,
    #[cfg(test)]
    pub(super) scanned_bytes: u64,
}

impl CachedCheckpointScan {
    pub(super) fn new(path: PathBuf, checkpoint: u64, metadata: &Metadata) -> Option<Self> {
        let observer = Observer::checkpoint_at(path, checkpoint).ok()?;
        let prefix_end = metadata
            .len()
            .min(checkpoint.saturating_add(BOUNDARY_BYTES));
        let checkpoint_prefix = Self::read_sample(&observer.path, checkpoint, prefix_end).ok()?;
        Some(Self {
            observer,
            device: metadata.dev(),
            inode: metadata.ino(),
            checkpoint_offset: checkpoint,
            checkpoint_prefix,
            observed_length: checkpoint,
            modified: metadata.modified().ok(),
            boundary: Vec::new(),
            #[cfg(test)]
            scanned_bytes: 0,
        })
    }

    pub(super) fn can_continue(&self, metadata: &Metadata) -> bool {
        if metadata.dev() != self.device
            || metadata.ino() != self.inode
            || metadata.len() < self.observed_length
        {
            return false;
        }
        if metadata.len() == self.observed_length && metadata.modified().ok() != self.modified {
            return false;
        }
        let prefix_end = self.checkpoint_offset + self.checkpoint_prefix.len() as u64;
        Self::read_sample(&self.observer.path, self.checkpoint_offset, prefix_end)
            .is_ok_and(|bytes| bytes == self.checkpoint_prefix)
            && Self::read_boundary(&self.observer.path, self.observer.offset)
                .is_ok_and(|bytes| bytes == self.boundary)
    }

    pub(super) fn scan_to(&mut self, length: u64) -> Result<(bool, bool), String> {
        #[cfg(test)]
        let start = self.observer.offset;
        self.observer.poll_to(length)?;
        #[cfg(test)]
        {
            self.scanned_bytes += length - start;
        }
        let status = (
            self.observer.evidence.started,
            self.observer.evidence.verified(None),
        );
        if self
            .observer
            .evidence
            .start_turn_id
            .as_ref()
            .is_some_and(|id| id.len() > MAX_CACHED_TURN_ID_BYTES)
        {
            return Err("Recovery turn ID exceeds the bounded scan cache".into());
        }
        // Time strings are only needed by the foreground recovery observer.
        // This cache evaluates lifecycle state, so keep no transcript-sized
        // strings between probes.
        self.observer.evidence.start_time = None;
        self.observer.evidence.work_time = None;
        if !self.observer.oversized {
            // Only complete lines affect evidence. Re-read a trailing partial
            // line on the next append instead of retaining up to MAX_LINE bytes.
            self.observer.offset -= self.observer.partial.len() as u64;
        }
        self.observer.partial.clear();
        self.observer.partial.shrink_to_fit();
        self.observed_length = length;
        self.boundary = Self::read_boundary(&self.observer.path, self.observer.offset)?;
        Ok(status)
    }

    pub(super) fn update_identity(&mut self, metadata: &Metadata) {
        self.modified = metadata.modified().ok();
    }

    fn read_boundary(path: &PathBuf, cursor: u64) -> Result<Vec<u8>, String> {
        let start = cursor.saturating_sub(BOUNDARY_BYTES);
        Self::read_sample(path, start, cursor)
    }

    fn read_sample(path: &PathBuf, start: u64, end: u64) -> Result<Vec<u8>, String> {
        let mut bytes = vec![0; (end - start) as usize];
        let mut file = File::open(path).map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(start))
            .map_err(|error| error.to_string())?;
        file.read_exact(&mut bytes)
            .map_err(|error| error.to_string())?;
        Ok(bytes)
    }
}
