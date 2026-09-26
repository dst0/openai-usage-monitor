use crate::models::{Settings, StatusFile};
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

use super::{
    accounts_registry_transaction_service::AccountsRegistryTransactionService, status_json_path,
};

pub fn write_status_file(status: &StatusFile) -> Result<(), String> {
    AccountsRegistryTransactionService::with_registry_locked(|registry| {
        let mut latest = status.clone();
        apply_settings(&mut latest, &registry.settings);
        write_status_file_unlocked(&latest)
    })
}

fn write_status_file_unlocked(status: &StatusFile) -> Result<(), String> {
    let path = status_json_path();
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
        #[cfg(unix)]
        {
            let _ = fs::set_permissions(parent, fs::Permissions::from_mode(0o700));
        }
    }

    let content = serde_json::to_vec_pretty(status)
        .map_err(|_| "Status file could not be encoded".to_string())?;
    let mut nonce = [0u8; 8];
    getrandom::getrandom(&mut nonce).map_err(|_| "Status staging nonce unavailable".to_string())?;
    let temp_path = path.with_extension(format!(
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
            .open(&temp_path)
            .map_err(|_| "Temporary status file could not be created safely".to_string())?;
        created = true;
        file.write_all(&content)
            .and_then(|_| file.sync_all())
            .map_err(|_| "Temporary status file could not be saved".to_string())?;
        fs::rename(&temp_path, &path).map_err(|_| "Status file replacement failed".to_string())
    })();
    if result.is_err() && created {
        let _ = fs::remove_file(&temp_path);
    }
    result
}

#[allow(dead_code)]
pub fn read_status_file() -> Result<StatusFile, String> {
    let path = status_json_path();
    let mut file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(&path)
        .map_err(|_| "Status file could not be opened safely".to_string())?;
    if !file
        .metadata()
        .map_err(|_| "Status file identity could not be read".to_string())?
        .is_file()
    {
        return Err("Status path is not a regular file".into());
    }
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|_| "Status file could not be read".to_string())?;
    let status: StatusFile = serde_json::from_str(&content)
        .map_err(|_| "Status file could not be parsed".to_string())?;
    Ok(status)
}

pub fn sync_settings_to_status_file() -> Result<(), String> {
    AccountsRegistryTransactionService::with_registry_locked(|registry| {
        let path = status_json_path();
        let mut status = match fs::symlink_metadata(path) {
            Ok(_) => read_status_file()?,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(()),
            Err(_) => return Err("Status file presence could not be checked".into()),
        };
        apply_settings(&mut status, &registry.settings);
        write_status_file_unlocked(&status)
    })
}

fn apply_settings(status: &mut StatusFile, settings: &Settings) {
    status.auto_switch_enabled = settings.auto_switch_enabled;
    status.auto_switch_business_only = settings.auto_switch_business_only;
    status.auto_switch_business_priority = settings.auto_switch_business_priority;
    status.auto_reset_weekly_enabled = settings.auto_reset_weekly_enabled;
    status.auto_reset_weekly_min_remaining_seconds =
        settings.auto_reset_weekly_min_remaining_seconds;
    if !settings.auto_reset_weekly_enabled {
        status.auto_reset_state = "disabled".to_string();
        status.auto_reset_reason = None;
    }
}

#[cfg(test)]
#[path = "status_file_service.test.rs"]
mod tests;
