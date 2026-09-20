use super::evidence::Evidence;
use std::{
    fs::File,
    io::{Read, Seek, SeekFrom},
    path::PathBuf,
};
pub(super) const MAX_LINE: usize = 131072;

pub(super) struct Observer {
    pub(super) path: PathBuf,
    pub(super) offset: u64,
    pub(super) partial: Vec<u8>,
    pub(super) oversized: bool,
    pub(super) evidence: Evidence,
}

impl Observer {
    pub(super) fn checkpoint(path: PathBuf) -> Result<Self, String> {
        let offset = path.metadata().map_err(|e| e.to_string())?.len();
        Self::checkpoint_at(path, offset)
    }

    pub(super) fn checkpoint_at(path: PathBuf, offset: u64) -> Result<Self, String> {
        let length = path.metadata().map_err(|e| e.to_string())?.len();
        if offset > length {
            return Err("Recovery checkpoint is past the end of its rollout".into());
        }
        Ok(Self {
            path,
            offset,
            partial: vec![],
            oversized: false,
            evidence: Evidence::default(),
        })
    }

    pub(super) fn poll(&mut self) -> Result<(), String> {
        let mut file = File::open(&self.path).map_err(|e| e.to_string())?;
        let length = file.metadata().map_err(|e| e.to_string())?.len();
        if length < self.offset {
            return Err("Rollout was truncated during recovery; cannot verify progress".into());
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
                break;
            }
            self.offset += n as u64;
            remaining -= n as u64;
            for byte in &buffer[..n] {
                if *byte == b'\n' {
                    if !self.oversized {
                        self.evidence.event(&self.partial);
                    }
                    self.partial.clear();
                    self.oversized = false;
                } else if !self.oversized {
                    if self.partial.len() == MAX_LINE {
                        self.partial.clear();
                        self.oversized = true;
                    } else {
                        self.partial.push(*byte);
                    }
                }
            }
        }
        Ok(())
    }
}
