use super::{acquire_switcher_lock, auth_json_path};
use crate::models::AuthJson;
use fs2::FileExt;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};

pub(super) struct ActiveAuthRemoveService;

pub(crate) fn compare_and_remove_active_auth_json(
    expected: &AuthJson,
    shared_auth_active: impl FnMut() -> Result<bool, String>,
) -> Result<(), String> {
    ActiveAuthRemoveService::remove_if_matches(expected, shared_auth_active)
}

impl ActiveAuthRemoveService {
    fn remove_if_matches(
        expected: &AuthJson,
        shared_auth_active: impl FnMut() -> Result<bool, String>,
    ) -> Result<(), String> {
        Self::remove_if_matches_with_hook(expected, shared_auth_active, || {})
    }

    fn remove_if_matches_with_hook(
        expected: &AuthJson,
        mut shared_auth_active: impl FnMut() -> Result<bool, String>,
        before_final_check: impl FnOnce(),
    ) -> Result<(), String> {
        let _lock = acquire_switcher_lock(true)?;
        if shared_auth_active()? {
            return Err("Shared credentials became active before first-switch rollback".into());
        }
        let path = auth_json_path();
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|_| "Active credential file could not be opened for rollback".to_string())?;
        file.lock_exclusive()
            .map_err(|_| "Active credential file could not be locked for rollback".to_string())?;
        let opened = file
            .metadata()
            .map_err(|_| "Active credential identity could not be read for rollback".to_string())?;
        if !opened.is_file() || opened.permissions().mode() & 0o777 != 0o600 {
            return Err("Active credential file is not a private regular file".into());
        }
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|_| "Active credential file could not be read for rollback".to_string())?;
        let observed: AuthJson = serde_json::from_str(&content)
            .map_err(|_| "Active credential file could not be parsed for rollback".to_string())?;
        if observed != *expected {
            return Err("Shared credentials changed before first-switch rollback".into());
        }
        before_final_check();
        let named = fs::symlink_metadata(&path)
            .map_err(|_| "Active credential identity changed before rollback".to_string())?;
        if !named.is_file() || named.dev() != opened.dev() || named.ino() != opened.ino() {
            return Err("Active credential identity changed before rollback".into());
        }
        let mut named_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|_| "Active credential file could not be reopened for rollback".to_string())?;
        let named_metadata = named_file
            .metadata()
            .map_err(|_| "Active credential identity changed before rollback".to_string())?;
        let mut named_content = String::new();
        named_file
            .read_to_string(&mut named_content)
            .map_err(|_| "Active credential file could not be reread for rollback".to_string())?;
        let named_auth: AuthJson = serde_json::from_str(&named_content)
            .map_err(|_| "Active credential file could not be parsed for rollback".to_string())?;
        if named_metadata.dev() != opened.dev()
            || named_metadata.ino() != opened.ino()
            || named_auth != *expected
            || named_content != content
        {
            return Err("Shared credentials changed before first-switch rollback".into());
        }
        if shared_auth_active()? {
            return Err("Shared credentials became active before first-switch rollback".into());
        }
        let final_named = fs::symlink_metadata(&path)
            .map_err(|_| "Active credential identity changed before rollback".to_string())?;
        if !final_named.is_file()
            || final_named.dev() != opened.dev()
            || final_named.ino() != opened.ino()
        {
            return Err("Active credential identity changed before rollback".into());
        }
        fs::remove_file(&path).map_err(|_| {
            "Active credential file could not be removed after failed first switch".to_string()
        })?;
        match fs::symlink_metadata(&path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
            _ => Err("Active credential file reappeared after first-switch rollback".into()),
        }
    }
}

#[cfg(test)]
#[path = "active_auth_remove_service.test.rs"]
mod tests;
