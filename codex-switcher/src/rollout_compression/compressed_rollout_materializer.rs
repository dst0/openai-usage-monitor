use super::file_identity::FileIdentity;
use super::materialize_outcome::MaterializeOutcome;
use super::rollout_file_name::RolloutFileName;
use super::rollout_writer_locks::RolloutWriterLocks;
use super::staging_file::StagingFile;
use super::zstd_rollout_codec::ZstdRolloutCodec;
use super::ROLLOUT_ROOTS;
use std::fs::{File, FileTimes, OpenOptions, Permissions};
use std::io;
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// Restores a compressed rollout to plain `.jsonl`, the way Codex does before
/// it appends, for tools that cannot read zstd.
///
/// Decoding runs unlocked into a staging file; publication holds writer
/// coordination with the thread idle, links the plain file into place
/// without clobbering, and removes the compressed file only if it is still
/// the one that was decoded.
pub struct CompressedRolloutMaterializer<'a> {
    codex_home: &'a Path,
    locks: &'a RolloutWriterLocks,
}

impl<'a> CompressedRolloutMaterializer<'a> {
    pub fn new(codex_home: &'a Path, locks: &'a RolloutWriterLocks) -> Self {
        Self { codex_home, locks }
    }

    /// Finds the thread's rollout in either root, plain or compressed.
    pub fn find(&self, thread_id: &str) -> Option<PathBuf> {
        let thread_id = thread_id.to_ascii_lowercase();
        for root in ROLLOUT_ROOTS {
            let mut stack = vec![self.codex_home.join(root)];
            while let Some(directory) = stack.pop() {
                let Ok(entries) = std::fs::read_dir(&directory) else {
                    continue;
                };
                for entry in entries.flatten() {
                    let Ok(file_type) = entry.file_type() else {
                        continue;
                    };
                    let path = entry.path();
                    if file_type.is_dir() {
                        stack.push(path);
                        continue;
                    }
                    let plain = RolloutFileName::plain_path(&path).unwrap_or_else(|| path.clone());
                    if file_type.is_file()
                        && RolloutFileName::from_path(&plain)
                            .is_some_and(|name| name.thread_id() == thread_id)
                    {
                        return Some(plain);
                    }
                }
            }
        }
        None
    }

    /// Restores `plain` from its compressed sibling.
    pub fn restore(&self, plain: &Path) -> io::Result<MaterializeOutcome> {
        let Some(name) = RolloutFileName::from_path(plain) else {
            return Ok(MaterializeOutcome::NotCompressed);
        };
        if FileIdentity::of_path(plain)?.is_some() {
            return Ok(MaterializeOutcome::AlreadyPlain);
        }
        let compressed = RolloutFileName::compressed_path(plain);
        let source = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&compressed)
        {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => {
                return Ok(MaterializeOutcome::NotCompressed)
            }
            Err(err) => return Err(err),
        };
        let before = FileIdentity::from_metadata(&source.metadata()?)?;
        let mut staging = StagingFile::create_beside(plain)?;
        let digest = ZstdRolloutCodec::decode(&source, staging.file_mut())?;
        let staged = staging.file();
        staged.set_permissions(Permissions::from_mode(before.mode & 0o7777))?;
        staged.set_times(FileTimes::new().set_modified(before.modified))?;
        staged.sync_all()?;

        let Some(_coordination) = self.locks.lock_coordination()? else {
            return Ok(MaterializeOutcome::Busy);
        };
        if !self.locks.thread_is_idle(name.thread_id())? {
            return Ok(MaterializeOutcome::Busy);
        }
        if !before.still_at(&compressed)? {
            return Ok(MaterializeOutcome::Changed);
        }
        if !staging.publish_noclobber(plain)? {
            return Ok(MaterializeOutcome::AlreadyPlain);
        }
        std::fs::remove_file(&compressed)?;
        if let Some(parent) = plain.parent() {
            File::open(parent)?.sync_all()?;
        }
        Ok(MaterializeOutcome::Restored { bytes: digest.len })
    }
}

/// Whether the thread's rollout exists only in compressed form.
pub fn is_compressed_thread(codex_home: &Path, thread_id: &str) -> bool {
    let locks = RolloutWriterLocks::new(codex_home);
    CompressedRolloutMaterializer::new(codex_home, &locks)
        .find(thread_id)
        .is_some_and(|plain| {
            !plain.exists() && RolloutFileName::compressed_path(&plain).is_file()
        })
}

#[cfg(test)]
#[path = "compressed_rollout_materializer.test.rs"]
mod tests;
