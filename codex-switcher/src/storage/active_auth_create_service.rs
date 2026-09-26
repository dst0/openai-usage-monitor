use super::{acquire_switcher_lock, auth_json_path};
use crate::models::AuthJson;
use std::fs::{self, OpenOptions};
use std::io::{ErrorKind, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub(super) struct ActiveAuthCreateService;

impl ActiveAuthCreateService {
    pub(super) fn create_if_absent(
        auth: &AuthJson,
        mut shared_auth_active: impl FnMut() -> Result<bool, String>,
    ) -> Result<(), String> {
        let _lock = acquire_switcher_lock(true)?;
        let path = auth_json_path();
        if shared_auth_active()? {
            return Err("Shared credentials became active before first switch commit".into());
        }
        match fs::symlink_metadata(&path) {
            Ok(_) => {
                return Err("Active credential file appeared before first switch commit".into())
            }
            Err(error) if error.kind() == ErrorKind::NotFound => {}
            Err(_) => return Err("Active credential presence could not be checked".into()),
        }
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Active credential staging nonce unavailable".to_string())?;
        let temp_path = path.with_extension(format!(
            "{}.{:016x}.tmp.json",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        ));
        let content = serde_json::to_vec_pretty(auth)
            .map_err(|_| "Active credential file could not be encoded".to_string())?;
        let mut created = false;
        let result = (|| {
            let mut staged = OpenOptions::new()
                .write(true)
                .create_new(true)
                .custom_flags(libc::O_NOFOLLOW)
                .mode(0o600)
                .open(&temp_path)
                .map_err(|_| "Temporary credential file could not be created".to_string())?;
            created = true;
            staged
                .write_all(&content)
                .and_then(|_| staged.sync_all())
                .map_err(|_| "Temporary credential file could not be saved".to_string())?;
            staged
                .set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "Temporary credential permissions could not be set".to_string())?;
            if shared_auth_active()? {
                return Err("Shared credentials became active before first switch commit".into());
            }
            fs::hard_link(&temp_path, &path).map_err(|_| {
                "Active credential file appeared before first switch commit".to_string()
            })
        })();
        if created {
            let _ = fs::remove_file(&temp_path);
        }
        result
    }
}
