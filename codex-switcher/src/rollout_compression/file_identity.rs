use std::fs::Metadata;
use std::io;
use std::os::unix::fs::MetadataExt;
use std::path::Path;
use std::time::SystemTime;

/// What must stay unchanged between reading a rollout and replacing it.
///
/// Codex compares length, modification time and permissions; the inode and
/// device also catch a file replaced by rename with identical metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileIdentity {
    pub dev: u64,
    pub ino: u64,
    pub len: u64,
    pub modified: SystemTime,
    pub mode: u32,
}

impl FileIdentity {
    pub fn from_metadata(metadata: &Metadata) -> io::Result<Self> {
        Ok(Self {
            dev: metadata.dev(),
            ino: metadata.ino(),
            len: metadata.len(),
            modified: metadata.modified()?,
            mode: metadata.mode(),
        })
    }

    /// Reads the identity of `path` without following a final symlink.
    /// Returns `None` when nothing is there.
    pub fn of_path(path: &Path) -> io::Result<Option<Self>> {
        match std::fs::symlink_metadata(path) {
            Ok(metadata) => Self::from_metadata(&metadata).map(Some),
            Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
            Err(err) => Err(err),
        }
    }

    /// Whether `path` still names exactly this file in this state.
    pub fn still_at(&self, path: &Path) -> io::Result<bool> {
        Ok(Self::of_path(path)?.as_ref() == Some(self))
    }
}
