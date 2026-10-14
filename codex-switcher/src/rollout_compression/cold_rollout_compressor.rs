use super::compression_checkpoints::CompressionCheckpoints;
use super::compression_outcome::CompressionOutcome;
use super::content_digest::ContentDigest;
use super::file_identity::FileIdentity;
use super::rollout_file_name::RolloutFileName;
use super::rollout_writer_locks::RolloutWriterLocks;
use super::session_meta_reader::SessionMetaReader;
use super::staging_file::StagingFile;
use super::zstd_rollout_codec::ZstdRolloutCodec;
use std::fs::{File, FileTimes, OpenOptions, Permissions};
use std::io::{self, Seek, SeekFrom};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Replaces one idle plain rollout with the `.jsonl.zst` Codex reads.
///
/// Encoding and verification run without locks. Publication then follows
/// Codex's compressor: hold writer coordination, require an idle thread lock,
/// recheck the source, link the frame into place without clobbering, recheck
/// again, and only then remove the source. Any change rolls back to the plain
/// file, which Codex always prefers.
pub struct ColdRolloutCompressor<'a, C: CompressionCheckpoints> {
    locks: &'a RolloutWriterLocks,
    checkpoints: &'a C,
    min_age: Duration,
    now: SystemTime,
}

impl<'a, C: CompressionCheckpoints> ColdRolloutCompressor<'a, C> {
    pub fn new(
        locks: &'a RolloutWriterLocks,
        checkpoints: &'a C,
        min_age: Duration,
        now: SystemTime,
    ) -> Self {
        Self {
            locks,
            checkpoints,
            min_age,
            now,
        }
    }

    pub fn compress(&self, plain: &Path) -> io::Result<CompressionOutcome> {
        let Some(name) = RolloutFileName::from_path(plain) else {
            return Ok(CompressionOutcome::NotRollout);
        };
        let compressed = RolloutFileName::compressed_path(plain);
        if FileIdentity::of_path(&compressed)?.is_some() {
            return Ok(CompressionOutcome::AlreadyCompressed);
        }
        let Some(mut source) = open_no_follow(plain)? else {
            return Ok(CompressionOutcome::NotRollout);
        };
        let metadata = source.metadata()?;
        if !metadata.is_file() {
            return Ok(CompressionOutcome::NotRollout);
        }
        let before = FileIdentity::from_metadata(&metadata)?;
        if self.now.duration_since(before.modified).unwrap_or_default() < self.min_age {
            return Ok(CompressionOutcome::NotCold);
        }
        if SessionMetaReader::thread_id(&source)?.as_deref() != Some(name.thread_id()) {
            return Ok(CompressionOutcome::MetadataMismatch);
        }
        source.seek(SeekFrom::Start(0))?;

        let mut staging = StagingFile::create_beside(&compressed)?;
        let digest = match ZstdRolloutCodec::encode(&mut source, before.len, staging.file_mut()) {
            Ok(digest) => digest,
            // An append or truncation during encoding is a change, not a fault.
            Err(_) if !before.still_at(plain)? => return Ok(CompressionOutcome::Changed),
            Err(err) => return Err(err),
        };
        staging.file().sync_all()?;
        verify(&mut staging, digest)?;
        self.checkpoints.after_encode(plain);
        if !before.still_at(plain)? {
            return Ok(CompressionOutcome::Changed);
        }
        let staged = staging.file();
        staged.set_permissions(Permissions::from_mode(before.mode & 0o7777))?;
        staged.set_times(FileTimes::new().set_modified(before.modified))?;
        staged.sync_all()?;
        let compressed_bytes = staged.metadata()?.len();

        let Some(_coordination) = self.locks.lock_coordination()? else {
            return Ok(CompressionOutcome::Busy);
        };
        if !self.locks.thread_is_idle(name.thread_id())? {
            return Ok(CompressionOutcome::Busy);
        }
        if !before.still_at(plain)? {
            return Ok(CompressionOutcome::Changed);
        }
        if !staging.publish_noclobber(&compressed)? {
            return Ok(CompressionOutcome::AlreadyCompressed);
        }
        self.checkpoints.before_remove_source(plain);
        if !before.still_at(plain)? {
            std::fs::remove_file(&compressed)?;
            return Ok(CompressionOutcome::Changed);
        }
        std::fs::remove_file(plain)?;
        sync_parent(plain)?;
        Ok(CompressionOutcome::Compressed {
            source_bytes: before.len,
            compressed_bytes,
        })
    }
}

/// Decodes the staged frame and requires exactly the bytes that were read.
fn verify(staging: &mut StagingFile, expected: ContentDigest) -> io::Result<()> {
    let file = staging.file_mut();
    file.seek(SeekFrom::Start(0))?;
    let decoded = ZstdRolloutCodec::decode(&*file, &mut io::sink())?;
    if decoded != expected {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "compressed rollout did not decode to the original bytes",
        ));
    }
    Ok(())
}

/// Opens a regular file for reading without following a final symlink.
/// `None` when the path is gone or is a symlink.
fn open_no_follow(path: &Path) -> io::Result<Option<File>> {
    match OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
    {
        Ok(file) => Ok(Some(file)),
        Err(err) if err.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(err) if err.raw_os_error() == Some(libc::ELOOP) => Ok(None),
        Err(err) => Err(err),
    }
}

fn sync_parent(path: &Path) -> io::Result<()> {
    match path.parent() {
        Some(parent) => File::open(parent)?.sync_all(),
        None => Ok(()),
    }
}

#[cfg(test)]
#[path = "cold_rollout_compressor.test.rs"]
mod tests;
