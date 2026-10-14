use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::Path;

/// Codex's home-wide lock for jobs that replace rollout files (its own
/// compressor and legacy-rollout migration). Holding it keeps those jobs and
/// this one from replacing files at the same time.
pub struct RolloutMaintenanceLock {
    _file: File,
}

impl RolloutMaintenanceLock {
    pub const RELATIVE_PATH: &'static str = ".tmp/rollout-maintenance.lock";

    /// Takes the lock without waiting. `None` means another maintenance job
    /// holds it.
    pub fn try_acquire(codex_home: &Path) -> io::Result<Option<Self>> {
        let path = codex_home.join(Self::RELATIVE_PATH);
        if let Some(directory) = path.parent() {
            std::fs::create_dir_all(directory)?;
        }
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(path)?;
        match file.try_lock_exclusive() {
            Ok(()) => Ok(Some(Self { _file: file })),
            Err(err) if is_contended(&err) => Ok(None),
            Err(err) => Err(err),
        }
    }
}

/// Whether a non-blocking `flock` failed only because another holder has it.
pub fn is_contended(err: &io::Error) -> bool {
    err.raw_os_error() == fs2::lock_contended_error().raw_os_error()
        || err.kind() == io::ErrorKind::WouldBlock
}
