use super::evidence::Evidence;
use std::{
    fs::{File, Metadata},
    io::{Read, Seek, SeekFrom},
    os::unix::fs::MetadataExt,
    path::PathBuf,
    time::SystemTime,
};
pub(super) const MAX_LINE: usize = 131072;

#[derive(Clone)]
pub(super) struct Observer {
    pub(super) path: PathBuf,
    pub(super) offset: u64,
    pub(super) partial: Vec<u8>,
    pub(super) oversized: bool,
    pub(super) saw_oversized: bool,
    pub(super) saw_malformed: bool,
    pub(super) evidence: Evidence,
    device: u64,
    inode: u64,
    observed_length: u64,
    scanned_any: bool,
    modified: Option<SystemTime>,
    changed_at: (i64, i64),
}

impl Observer {
    pub(super) fn checkpoint(path: PathBuf) -> Result<Self, String> {
        let offset = path.metadata().map_err(|e| e.to_string())?.len();
        Self::checkpoint_at(path, offset)
    }

    pub(super) fn checkpoint_at(path: PathBuf, offset: u64) -> Result<Self, String> {
        let metadata = path.metadata().map_err(|e| e.to_string())?;
        let length = metadata.len();
        if offset > length {
            return Err("Recovery checkpoint is past the end of its rollout".into());
        }
        Ok(Self {
            path,
            offset,
            partial: vec![],
            oversized: false,
            saw_oversized: false,
            saw_malformed: false,
            evidence: Evidence::default(),
            device: metadata.dev(),
            inode: metadata.ino(),
            observed_length: length,
            scanned_any: false,
            modified: metadata.modified().ok(),
            changed_at: (metadata.ctime(), metadata.ctime_nsec()),
        })
    }

    #[cfg(test)]
    pub(super) fn poll(&mut self) -> Result<(), String> {
        self.poll_with_limit(None)
    }

    pub(super) fn poll_to(&mut self, limit: u64) -> Result<(), String> {
        self.poll_with_limit(Some(limit))
    }

    /// Before dispatch, a later append could conceal a rewrite in the bytes
    /// already scanned. Only a fresh scan from the saved checkpoint can then
    /// prove that no user turn started, so this attempt retains the checkpoint.
    pub(super) fn pre_dispatch_snapshot_matches(&self, metadata: &Metadata) -> bool {
        !self.scanned_any
            || (metadata.dev() == self.device
                && metadata.ino() == self.inode
                && metadata.len() == self.observed_length
                && metadata.modified().ok() == self.modified
                && (metadata.ctime(), metadata.ctime_nsec()) == self.changed_at)
    }

    fn poll_with_limit(&mut self, limit: Option<u64>) -> Result<(), String> {
        let start_offset = self.offset;
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let metadata = file.metadata().map_err(|e| e.to_string())?;
        let current_length = metadata.len();
        if metadata.dev() != self.device
            || metadata.ino() != self.inode
            || current_length < self.observed_length
            || (current_length == self.observed_length
                && (metadata.modified().ok() != self.modified
                    || (metadata.ctime(), metadata.ctime_nsec()) != self.changed_at))
        {
            return Err("Rollout identity changed during recovery".into());
        }
        if current_length < self.offset || limit.is_some_and(|limit| current_length < limit) {
            return Err("Rollout was truncated during recovery; cannot verify progress".into());
        }
        let length = limit.unwrap_or(current_length);
        if length < self.offset {
            return Err("Recovery snapshot precedes its checkpoint".into());
        }
        file.seek(SeekFrom::Start(self.offset))
            .map_err(|e| e.to_string())?;
        // Snapshot length avoids chasing a live writer forever. Bounded chunks
        // preserve lifecycle records even when one tool result exceeds 128 KB.
        let mut remaining = length - self.offset;
        let mut buffer = [0_u8; 8192];
        while remaining > 0 {
            let take = remaining.min(buffer.len() as u64) as usize;
            let n = file.read(&mut buffer[..take]).map_err(|e| e.to_string())?;
            if n == 0 {
                return Err(
                    "Rollout ended before the recovery snapshot; cannot verify progress".into(),
                );
            }
            self.offset += n as u64;
            remaining -= n as u64;
            for byte in &buffer[..n] {
                if *byte == b'\n' {
                    if !self.oversized && !self.evidence.event(&self.partial) {
                        self.saw_malformed = true;
                    }
                    self.partial.clear();
                    self.oversized = false;
                } else if !self.oversized {
                    if self.partial.len() == MAX_LINE {
                        self.partial.clear();
                        self.oversized = true;
                        self.saw_oversized = true;
                    } else {
                        self.partial.push(*byte);
                    }
                }
            }
        }
        let after = file.metadata().map_err(|e| e.to_string())?;
        if after.dev() != self.device
            || after.ino() != self.inode
            || after.len() < current_length
            || (after.len() == current_length
                && (after.modified().ok() != metadata.modified().ok()
                    || (after.ctime(), after.ctime_nsec())
                        != (metadata.ctime(), metadata.ctime_nsec())))
        {
            return Err("Rollout changed while recovery was reading it".into());
        }
        self.observed_length = after.len();
        self.scanned_any |= self.offset > start_offset;
        self.modified = after.modified().ok();
        self.changed_at = (after.ctime(), after.ctime_nsec());
        Ok(())
    }
}
