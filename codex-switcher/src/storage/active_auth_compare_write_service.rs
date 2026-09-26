use crate::models::AuthJson;
use fs2::FileExt;
use serde_json::Value;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};

/// Compare-and-replace for the Desktop's shared auth file. The caller holds
/// codex.lock so Monitor writers cooperate; the Desktop does not take that
/// lock, so its races are detected as close to rename as the filesystem permits.
pub(super) struct ActiveAuthCompareWriteService<'a> {
    path: &'a Path,
}

impl<'a> ActiveAuthCompareWriteService<'a> {
    pub(super) fn new(path: &'a Path) -> Self {
        Self { path }
    }

    pub(super) fn execute(
        &self,
        expected: &AuthJson,
        replacement: &AuthJson,
        shared_auth_active: impl FnMut() -> Result<bool, String>,
    ) -> Result<(), String> {
        self.execute_with_hook(expected, replacement, shared_auth_active, || {})
    }

    pub(super) fn execute_replacing_tokens(
        &self,
        expected: &AuthJson,
        replacement: &AuthJson,
        shared_auth_active: impl FnMut() -> Result<bool, String>,
    ) -> Result<(), String> {
        self.execute_with_token_policy(expected, replacement, shared_auth_active, || {}, true)
    }

    fn execute_with_hook(
        &self,
        expected: &AuthJson,
        replacement: &AuthJson,
        shared_auth_active: impl FnMut() -> Result<bool, String>,
        before_final_check: impl FnOnce(),
    ) -> Result<(), String> {
        self.execute_with_token_policy(
            expected,
            replacement,
            shared_auth_active,
            before_final_check,
            false,
        )
    }

    fn execute_with_token_policy(
        &self,
        expected: &AuthJson,
        replacement: &AuthJson,
        mut shared_auth_active: impl FnMut() -> Result<bool, String>,
        before_final_check: impl FnOnce(),
        replace_tokens_entirely: bool,
    ) -> Result<(), String> {
        if shared_auth_active()? {
            return Err("Shared credentials became active before re-login commit".into());
        }
        let mut current_file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(self.path)
            .map_err(|_| "Active credential file could not be opened safely".to_string())?;
        current_file
            .lock_exclusive()
            .map_err(|_| "Active credential file could not be locked".to_string())?;
        let original_metadata = current_file
            .metadata()
            .map_err(|_| "Active credential identity could not be read".to_string())?;
        if !original_metadata.is_file() {
            return Err("Active credential path is not a regular file".into());
        }
        let (baseline, baseline_raw) = read_auth(&mut current_file)?;
        if !same_auth(&baseline, expected) {
            return Err("Shared credentials changed before re-login commit".into());
        }

        let temporary = self.temporary_path()?;
        let result = (|| {
            let replacement_raw = replace_known_auth_fields(
                baseline_raw.clone(),
                replacement,
                replace_tokens_entirely,
            )?;
            let content = serde_json::to_vec_pretty(&replacement_raw)
                .map_err(|_| "Replacement credentials could not be encoded".to_string())?;
            let mut staged = OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&temporary)
                .map_err(|_| "Temporary credential file could not be created".to_string())?;
            staged
                .write_all(&content)
                .and_then(|_| staged.sync_all())
                .map_err(|_| "Temporary credential file could not be saved".to_string())?;
            before_final_check();

            if shared_auth_active()? {
                return Err("Shared credentials became active before re-login commit".into());
            }
            let path_metadata = fs::symlink_metadata(self.path).map_err(|_| {
                "Active credential identity changed before re-login commit".to_string()
            })?;
            if !path_metadata.is_file()
                || path_metadata.dev() != original_metadata.dev()
                || path_metadata.ino() != original_metadata.ino()
            {
                return Err("Active credential identity changed before re-login commit".into());
            }
            let mut named_file = OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(self.path)
                .map_err(|_| {
                    "Active credential identity changed before re-login commit".to_string()
                })?;
            let named_metadata = named_file.metadata().map_err(|_| {
                "Active credential identity changed before re-login commit".to_string()
            })?;
            let (named_auth, named_raw) = read_auth(&mut named_file)?;
            if named_metadata.dev() != original_metadata.dev()
                || named_metadata.ino() != original_metadata.ino()
                || !same_auth(&named_auth, expected)
                || named_raw != baseline_raw
            {
                return Err("Shared credentials changed before re-login commit".into());
            }
            let final_path_metadata = fs::symlink_metadata(self.path).map_err(|_| {
                "Active credential identity changed before re-login commit".to_string()
            })?;
            if !final_path_metadata.is_file()
                || final_path_metadata.dev() != original_metadata.dev()
                || final_path_metadata.ino() != original_metadata.ino()
            {
                return Err("Active credential identity changed before re-login commit".into());
            }
            if shared_auth_active()? {
                return Err("Shared credentials became active before re-login commit".into());
            }
            fs::rename(&temporary, self.path)
                .map_err(|_| "Active credential replacement failed".to_string())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temporary);
        }
        result
    }

    fn temporary_path(&self) -> Result<PathBuf, String> {
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Credential staging nonce unavailable".to_string())?;
        let mut name = self.path.as_os_str().to_os_string();
        name.push(format!(
            ".{}.{:016x}.tmp",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        ));
        Ok(PathBuf::from(name))
    }
}

fn read_auth(file: &mut File) -> Result<(AuthJson, Value), String> {
    let mut content = String::new();
    file.read_to_string(&mut content)
        .map_err(|_| "Active credential file could not be read".to_string())?;
    let raw: Value = serde_json::from_str(&content)
        .map_err(|_| "Active credential file could not be parsed".to_string())?;
    let auth = serde_json::from_value(raw.clone())
        .map_err(|_| "Active credential file could not be parsed".to_string())?;
    Ok((auth, raw))
}

fn replace_known_auth_fields(
    mut raw: Value,
    replacement: &AuthJson,
    replace_tokens_entirely: bool,
) -> Result<Value, String> {
    let incoming = serde_json::to_value(replacement)
        .map_err(|_| "Replacement credentials could not be encoded".to_string())?;
    let old = raw
        .as_object_mut()
        .ok_or("Active credential root is not an object")?;
    let new = incoming
        .as_object()
        .ok_or("Replacement credential root is not an object")?;
    for key in ["auth_mode", "OPENAI_API_KEY", "last_refresh"] {
        old.remove(key);
        if let Some(value) = new.get(key) {
            old.insert(key.to_string(), value.clone());
        }
    }
    for (key, value) in new {
        if !["auth_mode", "OPENAI_API_KEY", "last_refresh", "tokens"].contains(&key.as_str()) {
            old.insert(key.clone(), value.clone());
        }
    }
    if replace_tokens_entirely {
        old.remove("tokens");
        if let Some(tokens) = new.get("tokens") {
            old.insert("tokens".into(), tokens.clone());
        }
        return Ok(raw);
    }
    match new.get("tokens") {
        Some(Value::Object(new_tokens)) => {
            let tokens = old
                .entry("tokens")
                .or_insert_with(|| Value::Object(Default::default()))
                .as_object_mut()
                .ok_or("Active credential tokens are not an object")?;
            for key in ["access_token", "refresh_token", "id_token", "account_id"] {
                tokens.remove(key);
                if let Some(value) = new_tokens.get(key) {
                    tokens.insert(key.to_string(), value.clone());
                }
            }
            for (key, value) in new_tokens {
                if !["access_token", "refresh_token", "id_token", "account_id"]
                    .contains(&key.as_str())
                {
                    tokens.insert(key.clone(), value.clone());
                }
            }
        }
        None => {
            old.remove("tokens");
        }
        Some(_) => return Err("Replacement credential tokens are invalid".into()),
    }
    Ok(raw)
}

fn same_auth(left: &AuthJson, right: &AuthJson) -> bool {
    left == right
}

#[cfg(test)]
#[path = "active_auth_compare_write_service.test.rs"]
mod tests;
