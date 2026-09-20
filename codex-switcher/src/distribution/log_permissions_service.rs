use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::monitor_log_io_service::MonitorLogIoService;
use super::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;

const PRIVATE_DIRECTORY_MODE: u32 = 0o700;
const PRIVATE_FILE_MODE: u32 = 0o600;

pub struct LogPermissionsService;

impl LogPermissionsService {
    pub fn enforce() -> Result<(), String> {
        Self::enforce_existing_home(&crate::storage::codex_home())
    }

    /// Installation initializes the owned log layout exactly once. Daemon
    /// startup deliberately uses `enforce` instead, so it cannot recreate a
    /// lifecycle inode that uninstall has removed.
    pub(crate) fn prepare_home(home: &Path) -> Result<(), String> {
        let home_dir = MonitorLogIoService::open_directory_path(home, false)
            .map_err(|_| Self::unsafe_path_error("Codex home"))?;
        let log_dir = Self::required_directory(&home_dir, "log", "log directory")?;
        let _archive_dir = Self::required_directory(&log_dir, "archive", "log archive")?;
        Self::ensure_private_file(&log_dir, "switcher.log", "switcher log")?;
        MonitorLogLifecycleLock::ensure(home)
            .map_err(|_| Self::unsafe_path_error("log lifecycle lock"))?;
        Self::ensure_private_file(&home_dir, "account-switcher-daemon.log", "daemon log")?;
        Self::ensure_private_file(&home_dir, "account-switcher-daemon.err", "daemon log")?;

        if let Some(recovery_dir) =
            MonitorLogIoService::open_child_directory(&home_dir, "recovery-runs", false)
                .map_err(|_| Self::unsafe_path_error("recovery directory"))?
        {
            recovery_dir
                .set_permissions(fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
                .map_err(|_| Self::unsafe_path_error("recovery directory"))?;
        }

        Ok(())
    }

    #[cfg(test)]
    pub(crate) fn enforce_home(home: &Path) -> Result<(), String> {
        Self::enforce_existing_home(home)
    }

    fn enforce_existing_home(home: &Path) -> Result<(), String> {
        let home_dir = MonitorLogIoService::open_directory_path(home, false)
            .map_err(|_| Self::unsafe_path_error("Codex home"))?;
        let log_dir = Self::existing_directory(&home_dir, "log", "log directory")?;
        let _archive_dir = Self::existing_directory(&log_dir, "archive", "log archive")?;
        Self::ensure_private_file(&log_dir, "switcher.log", "switcher log")?;
        Self::require_private_file(
            &log_dir,
            ".monitor-log-lifecycle.lock",
            "log lifecycle lock",
        )?;
        Self::ensure_private_file(&home_dir, "account-switcher-daemon.log", "daemon log")?;
        Self::ensure_private_file(&home_dir, "account-switcher-daemon.err", "daemon log")?;

        if let Some(recovery_dir) =
            MonitorLogIoService::open_child_directory(&home_dir, "recovery-runs", false)
                .map_err(|_| Self::unsafe_path_error("recovery directory"))?
        {
            recovery_dir
                .set_permissions(fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
                .map_err(|_| Self::unsafe_path_error("recovery directory"))?;
        }
        Ok(())
    }

    fn required_directory(
        parent: &std::fs::File,
        name: &str,
        error_kind: &str,
    ) -> Result<std::fs::File, String> {
        let directory = MonitorLogIoService::open_child_directory(parent, name, true)
            .map_err(|_| Self::unsafe_path_error(error_kind))?
            .ok_or_else(|| Self::unsafe_path_error(error_kind))?;
        directory
            .set_permissions(fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|_| Self::unsafe_path_error(error_kind))?;
        Ok(directory)
    }

    fn existing_directory(
        parent: &std::fs::File,
        name: &str,
        error_kind: &str,
    ) -> Result<std::fs::File, String> {
        let directory = MonitorLogIoService::open_child_directory(parent, name, false)
            .map_err(|_| Self::unsafe_path_error(error_kind))?
            .ok_or_else(|| Self::unsafe_path_error(error_kind))?;
        directory
            .set_permissions(fs::Permissions::from_mode(PRIVATE_DIRECTORY_MODE))
            .map_err(|_| Self::unsafe_path_error(error_kind))?;
        Ok(directory)
    }

    fn ensure_private_file(
        parent: &std::fs::File,
        name: &str,
        error_kind: &str,
    ) -> Result<(), String> {
        let file = match MonitorLogIoService::open_child_file(
            parent,
            name,
            libc::O_RDONLY | libc::O_CLOEXEC,
        ) {
            Ok(file) => file,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(Self::unsafe_path_error(error_kind)),
        };
        if !file
            .metadata()
            .map_err(|_| Self::unsafe_path_error(error_kind))?
            .is_file()
        {
            return Err(Self::unsafe_path_error(error_kind));
        }

        file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))
            .map_err(|_| Self::unsafe_path_error(error_kind))
    }

    fn require_private_file(
        parent: &std::fs::File,
        name: &str,
        error_kind: &str,
    ) -> Result<(), String> {
        let file =
            MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC)
                .map_err(|_| Self::unsafe_path_error(error_kind))?;
        file.set_permissions(fs::Permissions::from_mode(PRIVATE_FILE_MODE))
            .map_err(|_| Self::unsafe_path_error(error_kind))
    }

    fn unsafe_path_error(kind: &str) -> String {
        format!("unsafe {kind} path")
    }
}

#[cfg(test)]
#[path = "log_permissions_service.test.rs"]
mod tests;
