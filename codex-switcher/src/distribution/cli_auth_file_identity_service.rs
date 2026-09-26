use crate::models::AccountConfig;
use crate::storage::{auth_json_path, read_active_auth_json};
use std::os::unix::fs::MetadataExt;

pub(super) struct CliAuthFileIdentityService;

impl CliAuthFileIdentityService {
    pub(super) fn verified_id(account: &AccountConfig) -> Option<String> {
        let path = auth_json_path();
        let before = std::fs::metadata(&path).ok()?;
        if !before.is_file() {
            return None;
        }
        let auth = read_active_auth_json().ok()?;
        let after = std::fs::metadata(path).ok()?;
        let same_file = |left: &std::fs::Metadata, right: &std::fs::Metadata| {
            left.dev() == right.dev()
                && left.ino() == right.ino()
                && left.mtime() == right.mtime()
                && left.mtime_nsec() == right.mtime_nsec()
                && left.len() == right.len()
        };
        if !same_file(&before, &after) || auth.tokens.as_ref()? != &account.tokens {
            return None;
        }
        Some(format!(
            "{}:{}:{}:{}:{}",
            after.dev(),
            after.ino(),
            after.mtime(),
            after.mtime_nsec(),
            after.len()
        ))
    }
}
