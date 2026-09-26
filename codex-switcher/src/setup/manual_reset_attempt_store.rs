use super::manual_reset_attempt::ManualResetAttempt;
use crate::storage::codex_home;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::PathBuf;

const MAX_JOURNAL_BYTES: u64 = 16 * 1024;

pub(super) struct ManualResetAttemptStore;

impl ManualResetAttemptStore {
    pub(super) fn path() -> PathBuf {
        codex_home().join("manual-reset-state.json")
    }

    pub(super) fn load() -> Result<Option<ManualResetAttempt>, String> {
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(Self::path())
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Manual reset attempt could not be opened safely".into()),
        };
        let metadata = file
            .metadata()
            .map_err(|_| "Manual reset attempt metadata is unavailable".to_string())?;
        // SAFETY: geteuid has no inputs and does not access Rust-managed memory.
        let expected_uid = unsafe { libc::geteuid() };
        if !metadata.is_file()
            || metadata.uid() != expected_uid
            || metadata.mode() & 0o777 != 0o600
            || metadata.len() > MAX_JOURNAL_BYTES
        {
            return Err("Manual reset attempt ownership, mode, or size is unsafe".into());
        }
        let mut content = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_JOURNAL_BYTES + 1)
            .read_to_end(&mut content)
            .map_err(|_| "Manual reset attempt could not be read".to_string())?;
        if content.len() as u64 > MAX_JOURNAL_BYTES {
            return Err("Manual reset attempt is oversized".into());
        }
        let attempt: ManualResetAttempt = serde_json::from_slice(&content)
            .map_err(|_| "Manual reset attempt is malformed".to_string())?;
        attempt.validate()?;
        Ok(Some(attempt))
    }

    /// This small state document is atomically replaced, not appended; Brotli
    /// would obscure crash-state reads and add work to every manual reset.
    pub(super) fn write(attempt: &ManualResetAttempt) -> Result<(), String> {
        attempt.validate()?;
        let path = Self::path();
        let parent = path.parent().ok_or("Manual reset attempt has no parent")?;
        fs::create_dir_all(parent)
            .map_err(|_| "Manual reset directory could not be created".to_string())?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|_| "Manual reset directory permissions could not be set".to_string())?;
        let content = serde_json::to_vec(attempt)
            .map_err(|_| "Manual reset attempt could not be encoded".to_string())?;
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Manual reset attempt nonce unavailable".to_string())?;
        let temporary = path.with_extension(format!(
            "{}.{:016x}.tmp.json",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        ));
        let mut created = false;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .custom_flags(libc::O_NOFOLLOW)
                .mode(0o600)
                .open(&temporary)
                .map_err(|_| "Manual reset staging file could not be created".to_string())?;
            created = true;
            file.write_all(&content)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Manual reset staging file could not be saved".to_string())?;
            fs::rename(&temporary, &path)
                .map_err(|_| "Manual reset attempt could not be replaced".to_string())?;
            File::open(parent)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "Manual reset attempt directory sync failed".to_string())?;
            Ok(())
        })();
        if result.is_err() && created {
            let _ = fs::remove_file(&temporary);
        }
        result
    }
}

#[cfg(test)]
#[path = "manual_reset_attempt_store.test.rs"]
mod tests;
