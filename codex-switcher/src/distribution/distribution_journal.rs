use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const JOURNAL_FILENAME: &str = "distribution-journal.json";
const MAX_JOURNAL_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributionJournal {
    pub operation_id: String,
    pub pid: u32,
    pub trigger: String,
    pub reason: String,
    pub target_app_id: Option<String>,
    pub target_cli_id: Option<String>,
    pub phase: String,
    pub started_at: String,
    pub updated_at: String,
}

impl DistributionJournal {
    pub fn journal_path(home: &Path) -> PathBuf {
        home.join(JOURNAL_FILENAME)
    }

    pub fn create(
        home: &Path,
        operation_id: &str,
        trigger: &str,
        reason: &str,
        target_app: Option<&str>,
        target_cli: Option<&str>,
    ) -> Result<Self, String> {
        let now = Utc::now().to_rfc3339();
        let journal = Self {
            operation_id: operation_id.to_string(),
            pid: std::process::id(),
            trigger: trigger.to_string(),
            reason: reason.to_string(),
            target_app_id: target_app.map(ToString::to_string),
            target_cli_id: target_cli.map(ToString::to_string),
            phase: "initialized".to_string(),
            started_at: now.clone(),
            updated_at: now,
        };
        journal.save(home)?;
        Ok(journal)
    }

    pub fn load(home: &Path) -> Result<Option<Self>, String> {
        let path = Self::journal_path(home);
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Distribution journal could not be opened safely".into()),
        };
        let opened = file
            .metadata()
            .map_err(|_| "Distribution journal metadata is unavailable".to_string())?;
        validate_private_file(&opened)?;
        if opened.len() > MAX_JOURNAL_BYTES {
            return Err("Distribution journal is oversized".into());
        }
        let mut content = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|_| "Distribution journal could not be read".to_string())?;
        if content.len() as u64 > MAX_JOURNAL_BYTES {
            return Err("Distribution journal is oversized".into());
        }
        let named = fs::symlink_metadata(&path)
            .map_err(|_| "Distribution journal changed during read".to_string())?;
        validate_private_file(&named)?;
        if named.dev() != opened.dev() || named.ino() != opened.ino() {
            return Err("Distribution journal changed during read".into());
        }
        serde_json::from_slice(&content)
            .map(Some)
            .map_err(|_| "Distribution journal is invalid".into())
    }

    pub fn save(&self, home: &Path) -> Result<(), String> {
        fs::create_dir_all(home).map_err(|_| "Distribution journal directory is unavailable")?;
        let path = Self::journal_path(home);
        let previous = checked_named_file(&path)?;
        let data = serde_json::to_vec_pretty(self)
            .map_err(|_| "Distribution journal could not be encoded".to_string())?;
        if data.len() as u64 > MAX_JOURNAL_BYTES {
            return Err("Distribution journal is oversized".into());
        }
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Distribution journal staging nonce unavailable".to_string())?;
        let tmp = home.join(format!(
            "distribution-journal.{}.{:016x}.tmp",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        ));
        let mut temporary_identity = None;
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .custom_flags(libc::O_NOFOLLOW)
                .mode(0o600)
                .open(&tmp)
                .map_err(|_| {
                    "Distribution journal staging file could not be created".to_string()
                })?;
            let created = file
                .metadata()
                .map_err(|_| "Distribution journal staging identity is unavailable".to_string())?;
            temporary_identity = Some((created.dev(), created.ino()));
            file.write_all(&data)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Distribution journal staging file could not be saved".to_string())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "Distribution journal staging mode could not be set".to_string())?;
            file.sync_all()
                .map_err(|_| "Distribution journal staging file could not be saved".to_string())?;
            let staged = file
                .metadata()
                .map_err(|_| "Distribution journal staging identity is unavailable".to_string())?;
            drop(file);
            let named_stage = fs::symlink_metadata(&tmp)
                .map_err(|_| "Distribution journal staging identity changed".to_string())?;
            validate_private_file(&named_stage)?;
            if named_stage.dev() != staged.dev() || named_stage.ino() != staged.ino() {
                return Err("Distribution journal staging identity changed".into());
            }
            if checked_named_file(&path)? != previous {
                return Err("Distribution journal changed before replacement".into());
            }
            fs::rename(&tmp, &path)
                .map_err(|_| "Distribution journal replacement failed".to_string())?;
            File::open(home)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "Distribution journal directory sync failed".to_string())?;
            Ok(())
        })();
        if result.is_err() {
            if let (Some(expected), Ok(named)) = (temporary_identity, fs::symlink_metadata(&tmp)) {
                if named.is_file() && (named.dev(), named.ino()) == expected {
                    let _ = fs::remove_file(&tmp);
                }
            }
        }
        result
    }

    pub fn update_phase(&mut self, home: &Path, phase: &str) -> Result<(), String> {
        self.phase = phase.to_string();
        self.updated_at = Utc::now().to_rfc3339();
        self.save(home)
    }

    pub fn clear(home: &Path) -> Result<(), String> {
        let path = Self::journal_path(home);
        if checked_named_file(&path)?.is_none() {
            return Ok(());
        }
        match fs::remove_file(path) {
            Ok(()) => File::open(home)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "Distribution journal directory sync failed".into()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(_) => Err("Distribution journal could not be cleared".into()),
        }
    }

    pub fn is_stale(&self, timeout: Duration) -> bool {
        // If the process is dead, it is definitely stale
        if !is_process_alive(self.pid) {
            return true;
        }
        // If elapsed time exceeds timeout, treat as stale
        if let Ok(started) = DateTime::parse_from_rfc3339(&self.started_at) {
            let elapsed = Utc::now().signed_duration_since(started.with_timezone(&Utc));
            if elapsed
                > chrono::Duration::from_std(timeout)
                    .unwrap_or_else(|_| chrono::Duration::seconds(300))
            {
                return true;
            }
        }
        false
    }

    pub fn permits_stale_cleanup(&self) -> bool {
        self.phase == "initialized"
    }
}

fn validate_private_file(metadata: &fs::Metadata) -> Result<(), String> {
    // SAFETY: geteuid has no inputs and does not access Rust-managed memory.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.uid() != uid || metadata.mode() & 0o777 != 0o600 {
        return Err("Distribution journal ownership, mode, or type is unsafe".into());
    }
    Ok(())
}

fn checked_named_file(path: &Path) -> Result<Option<(u64, u64)>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_private_file(&metadata)?;
            Ok(Some((metadata.dev(), metadata.ino())))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Distribution journal path could not be inspected".into()),
    }
}

fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        // kill(pid, 0) returns 0 if process exists, or -1 with ESRCH if it does not
        if libc::kill(pid as i32, 0) == 0 {
            true
        } else {
            std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        }
    }
}

#[cfg(test)]
#[path = "distribution_journal.test.rs"]
mod tests;
