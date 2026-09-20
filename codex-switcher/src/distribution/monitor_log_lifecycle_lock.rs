use super::monitor_log_io_service::MonitorLogIoService;
use fs2::FileExt;
use std::fs::{self, File};
use std::io;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};

const LOCK_NAME: &str = ".monitor-log-lifecycle.lock";

pub(crate) struct MonitorLogLifecycleLock {
    file: File,
}

impl MonitorLogLifecycleLock {
    /// Creates the coordination inode during installation. Writers intentionally
    /// do not create it: after uninstall removes the inode, a stale writer must
    /// fail instead of silently creating an unrelated replacement lock.
    pub(crate) fn ensure(home: &Path) -> io::Result<()> {
        let file = Self::open(home, true)?;
        Self::validate(&file)
    }

    pub(crate) fn shared_for_log(path: &Path) -> io::Result<Self> {
        Self::shared(&Self::home_for_log(path)?)
    }

    pub(crate) fn shared(home: &Path) -> io::Result<Self> {
        Self::acquire(home, false)
    }

    pub(crate) fn exclusive(home: &Path) -> io::Result<Self> {
        Self::acquire(home, true)
    }

    fn acquire(home: &Path, exclusive: bool) -> io::Result<Self> {
        Self::acquire_with_before_lock(home, exclusive, || {})
    }

    pub(super) fn acquire_with_before_lock<F>(
        home: &Path,
        exclusive: bool,
        before_lock: F,
    ) -> io::Result<Self>
    where
        F: FnOnce(),
    {
        let file = Self::open(home, false)?;
        Self::validate(&file)?;
        before_lock();
        if exclusive {
            file.lock_exclusive()?;
        } else {
            file.lock_shared()?;
        }
        if let Err(error) = Self::verify_current_inode(home, &file) {
            let _ = file.unlock();
            return Err(error);
        }
        Ok(Self { file })
    }

    fn open(home: &Path, create: bool) -> io::Result<File> {
        let mut flags = libc::O_RDWR | libc::O_NOFOLLOW | libc::O_CLOEXEC;
        if create {
            flags |= libc::O_CREAT;
        }
        MonitorLogIoService::open_path(&home.join("log").join(LOCK_NAME), flags, 0o600, create)
    }

    fn validate(file: &File) -> io::Result<()> {
        if !file.metadata()?.is_file() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "log lifecycle lock is not a regular file",
            ));
        }
        file.set_permissions(fs::Permissions::from_mode(0o600))
    }

    fn verify_current_inode(home: &Path, held: &File) -> io::Result<()> {
        let current = Self::open(home, false)?;
        Self::validate(&current)?;
        let held_metadata = held.metadata()?;
        let current_metadata = current.metadata()?;
        if held_metadata.dev() != current_metadata.dev()
            || held_metadata.ino() != current_metadata.ino()
        {
            return Err(io::Error::other("log lifecycle lock changed while waiting"));
        }
        Ok(())
    }

    fn home_for_log(path: &Path) -> io::Result<PathBuf> {
        let parent = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log has no parent"))?;
        if parent.file_name().and_then(|name| name.to_str()) == Some("log") {
            return parent
                .parent()
                .map(Path::to_path_buf)
                .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "log has no home"));
        }
        Ok(parent.to_path_buf())
    }
}

impl Drop for MonitorLogLifecycleLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}
