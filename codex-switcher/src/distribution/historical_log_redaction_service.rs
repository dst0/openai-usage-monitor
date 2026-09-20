use super::historical_log_stream_redactor::{rewrite_brotli, rewrite_plain};
use super::monitor_log_io_service::MonitorLogIoService;
use super::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use super::monitor_log_name_policy::{is_owned_archive, is_recovery_log};
use super::temporary_log_rewrite::TemporaryLogRewrite;
use fs2::FileExt;
use std::ffi::CString;
use std::fs::{self, File};
use std::io;
use std::os::fd::AsRawFd;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub struct HistoricalLogRedactionService;

impl HistoricalLogRedactionService {
    pub fn sanitize_home(home: &Path) -> Result<(), String> {
        let _lifecycle = MonitorLogLifecycleLock::exclusive(home)
            .map_err(|_| Self::failure("log lifecycle lock"))?;
        for path in [
            home.join("log/switcher.log"),
            home.join("account-switcher-daemon.log"),
            home.join("account-switcher-daemon.err"),
        ] {
            Self::rewrite_path(&path, false).map_err(|_| Self::failure("active log"))?;
        }

        let archive_path = home.join("log/archive");
        let archive = MonitorLogIoService::open_directory_path(&archive_path, false)
            .map_err(|_| Self::failure("archive directory"))?;
        for name in MonitorLogIoService::archive_names(&archive)
            .map_err(|_| Self::failure("archive directory"))?
        {
            if is_owned_archive(&name) {
                Self::rewrite_child(&archive, &name, true)
                    .map_err(|_| Self::failure("Brotli archive"))?;
            }
        }
        let recovery_path = home.join("recovery-runs");
        match MonitorLogIoService::open_directory_path(&recovery_path, false) {
            Ok(recovery) => {
                for name in MonitorLogIoService::archive_names(&recovery)
                    .map_err(|_| Self::failure("recovery directory"))?
                {
                    if is_recovery_log(&name) {
                        Self::rewrite_child(&recovery, &name, false)
                            .map_err(|_| Self::failure("recovery log"))?;
                    }
                }
            }
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(Self::failure("recovery directory")),
        }
        Ok(())
    }

    fn rewrite_path(path: &Path, compressed: bool) -> io::Result<()> {
        let parent_path = path
            .parent()
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "missing parent"))?;
        let parent = MonitorLogIoService::open_directory_path(parent_path, false)?;
        let name = path
            .file_name()
            .and_then(|value| value.to_str())
            .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid filename"))?;
        Self::rewrite_child(&parent, name, compressed)
    }

    fn rewrite_child(parent: &File, name: &str, compressed: bool) -> io::Result<()> {
        let source =
            MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC)?;
        source.try_lock_exclusive()?;
        source.set_permissions(fs::Permissions::from_mode(0o600))?;
        let source_metadata = source.metadata()?;
        let reader_source = source.try_clone()?;
        // Clone the cleanup anchor before creating a temporary file. Once the
        // file exists, construction of its RAII cleanup guard is infallible.
        let cleanup_parent = parent.try_clone()?;
        let (temporary_name, mut temporary) = Self::create_temporary(parent, name)?;
        let mut cleanup = TemporaryLogRewrite::new(cleanup_parent, temporary_name.clone());
        temporary.set_permissions(fs::Permissions::from_mode(0o600))?;
        let changed = if compressed {
            rewrite_brotli(reader_source, &mut temporary)
        } else {
            rewrite_plain(reader_source, &mut temporary)
        }?;
        temporary.sync_all()?;
        drop(temporary);

        if !changed {
            cleanup.remove()?;
            let _ = source.unlock();
            return Ok(());
        }

        let final_source_metadata = source.metadata()?;
        if final_source_metadata.len() != source_metadata.len()
            || final_source_metadata.mtime() != source_metadata.mtime()
            || final_source_metadata.mtime_nsec() != source_metadata.mtime_nsec()
        {
            let _ = source.unlock();
            return Err(io::Error::other("log changed during redaction"));
        }

        let current =
            MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC)?;
        let current_metadata = current.metadata()?;
        if current_metadata.dev() != source_metadata.dev()
            || current_metadata.ino() != source_metadata.ino()
        {
            let _ = source.unlock();
            return Err(io::Error::other("log changed during redaction"));
        }
        drop(current);

        Self::rename_child(parent, &temporary_name, name)?;
        cleanup.disarm();
        let _ = source.unlock();
        parent.sync_all()
    }

    fn create_temporary(parent: &File, name: &str) -> io::Result<(String, File)> {
        for _ in 0..100 {
            let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let temporary_name = format!(".redact-{}-{sequence}.tmp", std::process::id());
            match MonitorLogIoService::open_child_file(
                parent,
                &temporary_name,
                libc::O_WRONLY | libc::O_CREAT | libc::O_EXCL | libc::O_CLOEXEC,
            ) {
                Ok(file) => return Ok((temporary_name, file)),
                Err(error) if error.kind() == io::ErrorKind::AlreadyExists => continue,
                Err(error) => return Err(error),
            }
        }
        Err(io::Error::new(
            io::ErrorKind::AlreadyExists,
            format!("could not allocate temporary file for {name}"),
        ))
    }

    fn rename_child(parent: &File, from: &str, to: &str) -> io::Result<()> {
        let from = CString::new(from)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid source name"))?;
        let to = CString::new(to)
            .map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "invalid target name"))?;
        let result = unsafe {
            libc::renameat(
                parent.as_raw_fd(),
                from.as_ptr(),
                parent.as_raw_fd(),
                to.as_ptr(),
            )
        };
        if result < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    fn failure(kind: &str) -> String {
        format!("could not safely redact pre-existing Monitor {kind}")
    }
}
