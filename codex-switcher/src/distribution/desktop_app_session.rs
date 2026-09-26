use super::window_restore_process_identity::ProcessIdentity;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::Path;

const MAX_SESSION_BYTES: u64 = 16 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopAppSession {
    pub account_id: String,
    pub updated_at: String,
    #[serde(default)]
    pub process: Option<ProcessIdentity>,
    #[serde(default)]
    pub cli_account_id: Option<String>,
}

impl DesktopAppSession {
    pub fn new(account_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            updated_at: Utc::now().to_rfc3339(),
            process: None,
            cli_account_id: None,
        }
    }

    pub fn bound(
        account_id: impl Into<String>,
        cli_account_id: impl Into<String>,
        process: ProcessIdentity,
    ) -> Self {
        let mut session = Self::new(account_id);
        session.process = Some(process);
        session.cli_account_id = Some(cli_account_id.into());
        session
    }

    pub fn load_checked(path: &Path) -> Result<Option<Self>, String> {
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Desktop session marker could not be opened safely".into()),
        };
        let opened = file
            .metadata()
            .map_err(|_| "Desktop session marker metadata is unavailable".to_string())?;
        validate_private_file(&opened)?;
        if opened.len() > MAX_SESSION_BYTES {
            return Err("Desktop session marker is oversized".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_SESSION_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Desktop session marker could not be read".to_string())?;
        if bytes.len() as u64 > MAX_SESSION_BYTES {
            return Err("Desktop session marker is oversized".into());
        }
        let named = fs::symlink_metadata(path)
            .map_err(|_| "Desktop session marker changed during read".to_string())?;
        validate_private_file(&named)?;
        if file_identity(&opened) != file_identity(&named)
            || opened.len() != named.len()
            || bytes.len() as u64 != opened.len()
        {
            return Err("Desktop session marker changed during read".into());
        }
        serde_json::from_slice(&bytes)
            .map(Some)
            .map_err(|_| "Desktop session marker is invalid".into())
    }

    pub fn load(path: &Path) -> Option<Self> {
        Self::load_checked(path).ok().flatten()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        self.save_with_post_rename(path, |parent| {
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "Desktop session directory sync failed".to_string())
        })
    }

    pub(super) fn save_with_post_rename(
        &self,
        path: &Path,
        post_rename: impl FnOnce(&Path) -> Result<(), String>,
    ) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or("Desktop session marker has no parent directory")?;
        fs::create_dir_all(parent).map_err(|_| "Desktop session directory is unavailable")?;
        let previous = checked_named_file(path)?;
        let data = serde_json::to_vec_pretty(self)
            .map_err(|_| "Desktop session marker could not be encoded".to_string())?;
        if data.len() as u64 > MAX_SESSION_BYTES {
            return Err("Desktop session marker is oversized".into());
        }
        let mut nonce = [0u8; 16];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Desktop session staging nonce unavailable".to_string())?;
        let tmp = parent.join(format!(
            "desktop-app-session.{}.{:032x}.tmp",
            std::process::id(),
            u128::from_ne_bytes(nonce)
        ));
        let mut file = OpenOptions::new()
            .create_new(true)
            .write(true)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(&tmp)
            .map_err(|_| "Desktop session staging file could not be created".to_string())?;
        let created = file
            .metadata()
            .map_err(|_| "Desktop session staging metadata is unavailable".to_string())?;
        let result = (|| {
            file.write_all(&data)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Desktop session staging file could not be saved".to_string())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "Desktop session staging mode could not be set".to_string())?;
            file.sync_all()
                .map_err(|_| "Desktop session staging file could not be saved".to_string())?;
            drop(file);
            if checked_named_file(&tmp)? != Some(file_identity(&created)) {
                return Err("Desktop session staging identity changed".into());
            }
            if checked_named_file(path)? != previous {
                return Err("Desktop session marker changed before replacement".into());
            }
            fs::rename(&tmp, path)
                .map_err(|_| "Desktop session marker replacement failed".to_string())?;
            post_rename(parent)
        })();
        if result.is_err()
            && fs::symlink_metadata(&tmp)
                .ok()
                .is_some_and(|named| file_identity(&named) == file_identity(&created))
        {
            let _ = fs::remove_file(&tmp);
        }
        result
    }

    /// A save error can occur after rename. Restore only the exact marker this
    /// operation wrote; a changed marker leaves the transaction journal intact.
    pub fn restore_after_failed_save(
        path: &Path,
        previous: Option<&Self>,
        attempted: &Self,
    ) -> Result<(), String> {
        let observed = Self::load_checked(path)?;
        if observed.as_ref() == previous {
            return Ok(());
        }
        if observed.as_ref() != Some(attempted) {
            return Err("Desktop session marker changed during rollback".into());
        }
        if let Some(previous) = previous {
            return previous.save(path);
        }
        let identity = checked_named_file(path)?
            .ok_or("Desktop session marker disappeared before rollback")?;
        if checked_named_file(path)? != Some(identity) {
            return Err("Desktop session marker changed before rollback".into());
        }
        fs::remove_file(path)
            .map_err(|_| "Desktop session marker could not be removed".to_string())?;
        File::open(
            path.parent()
                .ok_or("Desktop session marker has no parent directory")?,
        )
        .and_then(|directory| directory.sync_all())
        .map_err(|_| "Desktop session marker rollback sync failed".to_string())
    }
}

fn validate_private_file(metadata: &fs::Metadata) -> Result<(), String> {
    // SAFETY: geteuid has no input pointers and returns this process's UID.
    let uid = unsafe { libc::geteuid() };
    if !metadata.is_file() || metadata.uid() != uid || metadata.mode() & 0o777 != 0o600 {
        return Err("Desktop session marker ownership, mode, or type is unsafe".into());
    }
    Ok(())
}

fn checked_named_file(path: &Path) -> Result<Option<(u64, u64)>, String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) => {
            validate_private_file(&metadata)?;
            Ok(Some(file_identity(&metadata)))
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(_) => Err("Desktop session marker path could not be inspected".into()),
    }
}

fn file_identity(metadata: &fs::Metadata) -> (u64, u64) {
    (metadata.dev(), metadata.ino())
}

#[cfg(test)]
#[path = "desktop_app_session.test.rs"]
mod tests;
