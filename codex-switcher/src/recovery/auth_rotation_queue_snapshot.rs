use super::{
    queue_snapshot::{pending_count, queue_revision},
    thread_identity::valid_id,
};
use serde::{Deserialize, Serialize};
use std::{fs, io::ErrorKind, os::unix::fs::MetadataExt, path::Path};

/// Queue state that must stay unchanged from before the first rollout offset
/// through the post-stop checkpoint. A missing database is distinct from a
/// present database whose revision happens to be zero.
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq, Eq)]
pub(crate) struct AuthRotationQueueSnapshot {
    pub(super) database_identity: Option<(u64, u64)>,
    pub(super) revision: u64,
    pub(super) pending: usize,
}

impl AuthRotationQueueSnapshot {
    pub(super) fn read(home: &Path, id: &str) -> Result<Self, String> {
        if !valid_id(id) {
            return Err("Invalid recovery thread ID before queue query".into());
        }
        let path = home.join("queue_1.sqlite");
        let before = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                Some((metadata.dev(), metadata.ino()))
            }
            Ok(_) => return Err("Recovery queue database path is unsafe".into()),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        let revision = queue_revision(home, id)?;
        let pending = pending_count(home, id)?;
        let final_revision = queue_revision(home, id)?;
        let after = match fs::symlink_metadata(&path) {
            Ok(metadata) if metadata.is_file() && !metadata.file_type().is_symlink() => {
                Some((metadata.dev(), metadata.ino()))
            }
            Ok(_) => return Err("Recovery queue database path became unsafe".into()),
            Err(error) if error.kind() == ErrorKind::NotFound => None,
            Err(error) => return Err(error.to_string()),
        };
        if before != after || revision != final_revision {
            return Err("Recovery queue changed while capturing its snapshot".into());
        }
        Ok(Self {
            database_identity: before,
            revision,
            pending,
        })
    }
}
