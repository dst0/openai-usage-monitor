use super::historical_log_redaction_service::HistoricalLogRedactionService;
use super::log_permissions_service::LogPermissionsService;
use super::monitor_log_io_service::MonitorLogIoService;
use super::monitor_log_name_policy::{is_owned_archive, is_redaction_temp};
use std::fs::File;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;

pub struct MonitorLogCleanupService;

impl MonitorLogCleanupService {
    pub fn cancel_recovery() -> Result<(), String> {
        let path = crate::storage::codex_home().join("recovery-runs/cancel-restart");
        let mut file = MonitorLogIoService::open_path(
            &path,
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC | libc::O_NOFOLLOW | libc::O_CLOEXEC,
            0o600,
            true,
        )
        .map_err(|_| Self::unsafe_error("Monitor recovery state"))?;
        use std::io::Write;
        file.write_all(b"cancel\n")
            .map_err(|_| Self::unsafe_error("Monitor recovery state"))?;
        file.sync_all()
            .map_err(|_| Self::unsafe_error("Monitor recovery state"))
    }

    pub fn install() -> Result<(), String> {
        let home = crate::storage::codex_home();
        MonitorLogIoService::open_directory_path(&home, true)
            .map_err(|_| Self::unsafe_error("Codex home"))?;
        LogPermissionsService::enforce_home(&home)?;
        for path in [
            home.join("log/switcher.log"),
            home.join("account-switcher-daemon.log"),
            home.join("account-switcher-daemon.err"),
        ] {
            let file = MonitorLogIoService::open_path(
                &path,
                libc::O_RDWR | libc::O_CREAT | libc::O_NOFOLLOW | libc::O_CLOEXEC,
                0o600,
                true,
            )
            .map_err(|_| Self::unsafe_error("Monitor log"))?;
            if !file
                .metadata()
                .map_err(|_| Self::unsafe_error("Monitor log"))?
                .is_file()
            {
                return Err(Self::unsafe_error("Monitor log"));
            }
            file.set_permissions(std::fs::Permissions::from_mode(0o600))
                .map_err(|_| Self::unsafe_error("Monitor log"))?;
        }
        HistoricalLogRedactionService::sanitize_home(&home)
    }

    pub fn print_plan(purge_data: bool) -> Result<(), String> {
        let home_path = crate::storage::codex_home();
        let home = match MonitorLogIoService::open_directory_path(&home_path, false) {
            Ok(home) => home,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(Self::unsafe_error("Codex home")),
        };
        let log_dir = Self::optional_directory(&home, "log")?;
        if let Some(log_dir) = log_dir {
            Self::print_regular_child(
                &log_dir,
                "switcher.log",
                &home_path.join("log/switcher.log"),
            )?;
            Self::print_temp_files(&log_dir, &home_path.join("log"))?;
            let archive = Self::optional_directory(&log_dir, "archive")?;
            if let Some(archive) = archive {
                let entries = MonitorLogIoService::archive_names(&archive)
                    .map_err(|_| Self::unsafe_error("Monitor archive"))?;
                for name in entries {
                    if is_owned_archive(&name) {
                        Self::require_regular_archive(&archive, &name)?;
                        println!(
                            "  remove {}",
                            home_path.join("log/archive").join(&name).display()
                        );
                    } else if Self::is_temp_file(&name) {
                        Self::print_regular_child(
                            &archive,
                            &name,
                            &home_path.join("log/archive").join(&name),
                        )?;
                    }
                }
                println!("  preserve foreign Brotli archives in the Monitor archive directory");
            }
        }
        for name in ["account-switcher-daemon.log", "account-switcher-daemon.err"] {
            Self::print_regular_child(&home, name, &home_path.join(name))?;
        }
        Self::print_temp_files(&home, &home_path)?;
        if Self::optional_directory(&home, "recovery-runs")?.is_some() {
            println!("  remove {}", home_path.join("recovery-runs").display());
        }
        if purge_data {
            Self::print_regular_child(&home, "accounts.json", &home_path.join("accounts.json"))?;
        }
        Ok(())
    }

    pub fn remove(purge_data: bool) -> Result<(), String> {
        let home_path = crate::storage::codex_home();
        let home = match MonitorLogIoService::open_directory_path(&home_path, false) {
            Ok(home) => home,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err(Self::unsafe_error("Codex home")),
        };

        Self::remove_regular_child(&home, "account-switcher-daemon.log")?;
        Self::remove_regular_child(&home, "account-switcher-daemon.err")?;
        Self::remove_temp_files(&home)?;

        if let Some(log_dir) = Self::optional_directory(&home, "log")? {
            Self::remove_regular_child(&log_dir, "switcher.log")?;
            Self::remove_temp_files(&log_dir)?;
            if let Some(archive) = Self::optional_directory(&log_dir, "archive")? {
                let entries = MonitorLogIoService::archive_names(&archive)
                    .map_err(|_| Self::unsafe_error("Monitor archive"))?;
                for name in entries {
                    if is_owned_archive(&name) {
                        Self::require_regular_archive(&archive, &name)?;
                        MonitorLogIoService::remove_child(&archive, &name, false)
                            .map_err(|_| Self::unsafe_error("Monitor archive"))?;
                    } else if Self::is_temp_file(&name) {
                        Self::remove_regular_child(&archive, &name)?;
                    }
                }
                let _ = MonitorLogIoService::remove_child(&log_dir, "archive", true);
            }
            let _ = MonitorLogIoService::remove_child(&home, "log", true);
        }

        if Self::optional_directory(&home, "recovery-runs")?.is_some() {
            Self::remove_tree(&home, "recovery-runs")?;
        }

        if purge_data {
            Self::remove_regular_child(&home, "accounts.json")?;
        }
        Ok(())
    }

    fn optional_directory(parent: &File, name: &str) -> Result<Option<File>, String> {
        MonitorLogIoService::open_child_directory(parent, name, false)
            .map_err(|_| Self::unsafe_error("Monitor directory"))
    }

    fn print_regular_child(parent: &File, name: &str, path: &Path) -> Result<(), String> {
        match MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC) {
            Ok(_) => println!("  remove {}", path.display()),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {}
            Err(_) => return Err(Self::unsafe_error("Monitor file")),
        }
        Ok(())
    }

    fn require_regular_archive(directory: &File, name: &str) -> Result<(), String> {
        MonitorLogIoService::open_archive_file(directory, name)
            .map(|_| ())
            .map_err(|_| Self::unsafe_error("Monitor archive"))
    }

    fn remove_regular_child(parent: &File, name: &str) -> Result<(), String> {
        match MonitorLogIoService::open_child_file(parent, name, libc::O_RDONLY | libc::O_CLOEXEC) {
            Ok(_) => MonitorLogIoService::remove_child(parent, name, false)
                .map_err(|_| Self::unsafe_error("Monitor file")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err(Self::unsafe_error("Monitor file")),
        }
    }

    fn print_temp_files(home: &File, home_path: &Path) -> Result<(), String> {
        for name in MonitorLogIoService::archive_names(home)
            .map_err(|_| Self::unsafe_error("Monitor state"))?
        {
            if Self::is_temp_file(&name) {
                Self::print_regular_child(home, &name, &home_path.join(&name))?;
            }
        }
        Ok(())
    }

    fn remove_temp_files(home: &File) -> Result<(), String> {
        for name in MonitorLogIoService::archive_names(home)
            .map_err(|_| Self::unsafe_error("Monitor state"))?
        {
            if Self::is_temp_file(&name) {
                Self::remove_regular_child(home, &name)?;
            }
        }
        Ok(())
    }

    fn remove_tree(parent: &File, name: &str) -> Result<(), String> {
        let directory = MonitorLogIoService::open_child_directory(parent, name, false)
            .map_err(|_| Self::unsafe_error("Monitor recovery state"))?
            .ok_or_else(|| Self::unsafe_error("Monitor recovery state"))?;
        for child in MonitorLogIoService::archive_names(&directory)
            .map_err(|_| Self::unsafe_error("Monitor recovery state"))?
        {
            match MonitorLogIoService::open_child_directory(&directory, &child, false) {
                Ok(Some(_)) => Self::remove_tree(&directory, &child)?,
                Ok(None) => {}
                Err(error) if error.kind() == io::ErrorKind::NotADirectory => {
                    Self::remove_regular_child(&directory, &child)?;
                }
                Err(_) => return Err(Self::unsafe_error("Monitor recovery state")),
            }
        }
        MonitorLogIoService::remove_child(parent, name, true)
            .map_err(|_| Self::unsafe_error("Monitor recovery state"))
    }

    fn is_temp_file(name: &str) -> bool {
        is_redaction_temp(name)
            || [
                ("auth.", ".tmp.json"),
                ("accounts.", ".tmp.json"),
                ("usage-status.", ".tmp.json"),
                ("desktop-recovery.", ".tmp"),
                ("desktop-automation-cooldown.", ".tmp"),
                ("desktop-window.", ".tmp"),
                (".auto-reset-state.", ".tmp"),
            ]
            .iter()
            .any(|(prefix, suffix)| {
                name.starts_with(prefix)
                    && name.ends_with(suffix)
                    && name.len() > prefix.len() + suffix.len()
            })
    }

    fn unsafe_error(kind: &str) -> String {
        format!("unsafe {kind} path")
    }
}
