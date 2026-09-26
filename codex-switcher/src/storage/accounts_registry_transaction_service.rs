use crate::models::AccountsFile;
use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use super::{accounts_json_path, acquire_switcher_lock};

/// Performs registry read/modify/write while holding one Monitor-wide lock.
/// The daemon uses this to merge quota data without replaying stale tokens.
pub(super) struct AccountsRegistryTransactionService;

impl AccountsRegistryTransactionService {
    pub(super) fn initialize_if_absent(initial: &AccountsFile) -> Result<AccountsFile, String> {
        let _lock = acquire_switcher_lock(true)?;
        match fs::symlink_metadata(accounts_json_path()) {
            Ok(_) => Self::read_unlocked(),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                Self::write_unlocked(initial)?;
                Ok(initial.clone())
            }
            Err(_) => Err("Accounts registry presence could not be checked".into()),
        }
    }

    #[cfg(test)]
    pub(super) fn save(accounts: &AccountsFile) -> Result<(), String> {
        let _lock = acquire_switcher_lock(true)?;
        Self::write_unlocked(accounts)
    }

    pub(super) fn update(
        update: impl FnOnce(&mut AccountsFile) -> Result<(), String>,
    ) -> Result<AccountsFile, String> {
        let _lock = acquire_switcher_lock(true)?;
        let mut accounts = Self::read_unlocked()?;
        update(&mut accounts)?;
        Self::write_unlocked(&accounts)?;
        Ok(accounts)
    }

    pub(super) fn with_registry_locked<T>(
        operation: impl FnOnce(&AccountsFile) -> Result<T, String>,
    ) -> Result<T, String> {
        let _lock = acquire_switcher_lock(true)?;
        let accounts = Self::read_unlocked()?;
        operation(&accounts)
    }

    fn read_unlocked() -> Result<AccountsFile, String> {
        let path = accounts_json_path();
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|_| "Accounts registry could not be opened safely".to_string())?;
        file.lock_shared()
            .map_err(|_| "Accounts registry could not be locked".to_string())?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|_| "Accounts registry could not be read".to_string())?;
        serde_json::from_str(&content)
            .map_err(|_| "Accounts registry could not be parsed".to_string())
    }

    fn write_unlocked(accounts: &AccountsFile) -> Result<(), String> {
        let path = accounts_json_path();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)
                .map_err(|_| "Accounts directory could not be created".to_string())?;
        }
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Accounts registry staging nonce unavailable".to_string())?;
        let temp_path = path.with_extension(format!(
            "{}.{:016x}.tmp.json",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        ));
        let content = serde_json::to_vec_pretty(accounts)
            .map_err(|_| "Accounts registry could not be encoded".to_string())?;
        let mut created = false;
        let result = (|| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .custom_flags(libc::O_NOFOLLOW)
                .mode(0o600)
                .open(&temp_path)
                .map_err(|_| "Temporary accounts registry could not be created".to_string())?;
            created = true;
            file.lock_exclusive()
                .map_err(|_| "Temporary accounts registry could not be locked".to_string())?;
            file.write_all(&content)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Temporary accounts registry could not be saved".to_string())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| {
                    "Temporary accounts registry permissions could not be set".to_string()
                })?;
            fs::rename(&temp_path, &path)
                .map_err(|_| "Accounts registry replacement failed".to_string())?;
            Ok(())
        })();
        if result.is_err() && created {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}

#[cfg(test)]
#[path = "accounts_registry_transaction_service.test.rs"]
mod tests;
