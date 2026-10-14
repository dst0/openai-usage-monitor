use super::rollout_maintenance_lock::is_contended;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Codex's cross-process writer ownership for threads.
///
/// Every Codex writer takes `thread-writer-locks/.coordination.lock` before it
/// opens and locks `<thread id>.lock`. A compressor that holds coordination
/// and finds the thread lock free therefore knows no writer can start until
/// it releases coordination; that is how Codex's own compressor publishes.
pub struct RolloutWriterLocks {
    directory: PathBuf,
    coordination_wait: Duration,
}

const DIRECTORY: &str = "thread-writer-locks";
const COORDINATION_FILE: &str = ".coordination.lock";
const POLL_INTERVAL: Duration = Duration::from_millis(20);

impl RolloutWriterLocks {
    /// Codex holds coordination only around lock-file changes and
    /// publication, so a longer wait means something is stuck: skip the file.
    pub const DEFAULT_COORDINATION_WAIT: Duration = Duration::from_secs(5);

    pub fn new(codex_home: &Path) -> Self {
        Self::with_coordination_wait(codex_home, Self::DEFAULT_COORDINATION_WAIT)
    }

    pub fn with_coordination_wait(codex_home: &Path, coordination_wait: Duration) -> Self {
        Self {
            directory: codex_home.join(DIRECTORY),
            coordination_wait,
        }
    }

    /// Takes the coordination lock, waiting at most the configured time.
    /// `None` means it stayed held for the whole wait.
    pub fn lock_coordination(&self) -> io::Result<Option<File>> {
        std::fs::create_dir_all(&self.directory)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(self.directory.join(COORDINATION_FILE))?;
        let started = Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(Some(file)),
                Err(err) if is_contended(&err) => {
                    if started.elapsed() >= self.coordination_wait {
                        return Ok(None);
                    }
                    std::thread::sleep(POLL_INTERVAL);
                }
                Err(err) => return Err(err),
            }
        }
    }

    /// Whether no writer owns the thread. Call only while holding
    /// coordination, so the answer stays true until it is released.
    ///
    /// The probe never creates a lock file, and it takes the lock only for
    /// the instant of the test: closing the descriptor releases it.
    pub fn thread_is_idle(&self, thread_id: &str) -> io::Result<bool> {
        let path = self.directory.join(format!("{thread_id}.lock"));
        let file = match OpenOptions::new().read(true).write(true).open(path) {
            Ok(file) => file,
            Err(err) if err.kind() == io::ErrorKind::NotFound => return Ok(true),
            Err(err) => return Err(err),
        };
        match file.try_lock_exclusive() {
            Ok(()) => Ok(true),
            Err(err) if is_contended(&err) => Ok(false),
            Err(err) => Err(err),
        }
    }
}

#[cfg(test)]
#[path = "rollout_writer_locks.test.rs"]
mod tests;
