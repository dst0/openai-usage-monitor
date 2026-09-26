use super::DirectSwitchJournal;
use std::fs::{self, File, OpenOptions};
use std::io::{ErrorKind, Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

const FILE_NAME: &str = "direct-switch-journal.json";
const MAX_BYTES: u64 = 16 * 1024;

pub(super) struct DirectSwitchJournalStore;

impl DirectSwitchJournalStore {
    fn path(home: &Path) -> PathBuf {
        home.join(FILE_NAME)
    }

    pub(super) fn load(home: &Path) -> Result<Option<DirectSwitchJournal>, String> {
        let path = Self::path(home);
        let mut file = match OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Direct switch intent could not be opened safely".into()),
        };
        let opened = file
            .metadata()
            .map_err(|_| "Direct switch intent metadata is unavailable".to_string())?;
        private_file(&opened)?;
        if opened.len() > MAX_BYTES {
            return Err("Direct switch intent is oversized".into());
        }
        let mut bytes = Vec::new();
        Read::by_ref(&mut file)
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|_| "Direct switch intent could not be read".to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Direct switch intent is oversized".into());
        }
        let named = fs::symlink_metadata(&path)
            .map_err(|_| "Direct switch intent changed during read".to_string())?;
        private_file(&named)?;
        if named.dev() != opened.dev() || named.ino() != opened.ino() {
            return Err("Direct switch intent changed during read".into());
        }
        let journal: DirectSwitchJournal = serde_json::from_slice(&bytes)
            .map_err(|_| "Direct switch intent is malformed".to_string())?;
        journal.validate()?;
        Ok(Some(journal))
    }

    pub(super) fn create(home: &Path, journal: &DirectSwitchJournal) -> Result<(), String> {
        journal.validate()?;
        fs::create_dir_all(home).map_err(|_| "Direct switch home is unavailable".to_string())?;
        let bytes = serde_json::to_vec(journal)
            .map_err(|_| "Direct switch intent could not be encoded".to_string())?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err("Direct switch intent is oversized".into());
        }
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Direct switch staging nonce unavailable".to_string())?;
        let temp = home.join(format!(
            "direct-switch-journal.{}.{:016x}.tmp",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        ));
        let path = Self::path(home);
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(&temp)
            .map_err(|_| "Direct switch staging file could not be created".to_string())?;
        let staged = file
            .metadata()
            .map_err(|_| "Direct switch staging identity is unavailable".to_string())?;
        let result = (|| {
            file.write_all(&bytes)
                .and_then(|_| file.sync_all())
                .map_err(|_| "Direct switch staging file could not be saved".to_string())?;
            file.set_permissions(fs::Permissions::from_mode(0o600))
                .map_err(|_| "Direct switch staging mode could not be set".to_string())?;
            file.sync_all()
                .map_err(|_| "Direct switch staging file could not be saved".to_string())?;
            let named_stage = fs::symlink_metadata(&temp)
                .map_err(|_| "Direct switch staging identity changed".to_string())?;
            private_file(&named_stage)?;
            if named_stage.dev() != staged.dev() || named_stage.ino() != staged.ino() {
                return Err("Direct switch staging identity changed".into());
            }
            fs::hard_link(&temp, &path)
                .map_err(|_| "A direct switch intent already exists".to_string())?;
            fs::remove_file(&temp)
                .map_err(|_| "Direct switch staging file could not be removed".to_string())?;
            File::open(home)
                .and_then(|directory| directory.sync_all())
                .map_err(|_| "Direct switch intent directory sync failed".to_string())?;
            if Self::load(home)?.as_ref() != Some(journal) {
                return Err("Direct switch intent readback failed".into());
            }
            Ok(())
        })();
        if result.is_err() {
            if let Ok(named) = fs::symlink_metadata(&temp) {
                if named.is_file() && named.dev() == staged.dev() && named.ino() == staged.ino() {
                    let _ = fs::remove_file(&temp);
                }
            }
        }
        result
    }

    pub(super) fn clear_if_matches(
        home: &Path,
        expected: &DirectSwitchJournal,
    ) -> Result<(), String> {
        if Self::load(home)?.as_ref() != Some(expected) {
            return Err("Direct switch intent changed before cleanup".into());
        }
        let path = Self::path(home);
        fs::remove_file(&path)
            .map_err(|_| "Direct switch intent could not be cleared".to_string())?;
        File::open(home)
            .and_then(|directory| directory.sync_all())
            .map_err(|_| "Direct switch intent directory sync failed".to_string())
    }
}

fn private_file(metadata: &fs::Metadata) -> Result<(), String> {
    // SAFETY: geteuid has no inputs and does not access Rust-managed memory.
    let expected_uid = unsafe { libc::geteuid() };
    if !metadata.is_file()
        || metadata.uid() != expected_uid
        || metadata.permissions().mode() & 0o777 != 0o600
    {
        return Err("Direct switch intent ownership, mode, or type is unsafe".into());
    }
    Ok(())
}
