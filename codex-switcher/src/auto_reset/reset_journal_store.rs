use super::reset_journal::{ResetJournal, JOURNAL_VERSION};
use crate::storage;
use std::fs::{self, OpenOptions};
use std::io::{Read, Write};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_IDEMPOTENCY_SEQUENCE: AtomicU64 = AtomicU64::new(1);

pub(super) struct ResetJournalStore;

impl ResetJournalStore {
    fn path() -> PathBuf {
        storage::codex_home().join("auto-reset-state.json")
    }

    pub(super) fn load_at(path: &Path) -> Result<ResetJournal, String> {
        let metadata = match fs::symlink_metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                return Ok(ResetJournal::default())
            }
            Err(error) => return Err(format!("Unable to inspect auto-reset journal: {error}")),
        };
        // Reset state controls a consumable account resource. Refuse a symlink,
        // foreign-owned file, or broad permissions instead of trusting it.
        // SAFETY: geteuid has no inputs and does not access Rust-managed memory.
        let expected_uid = unsafe { libc::geteuid() };
        if !metadata.file_type().is_file()
            || metadata.uid() != expected_uid
            || metadata.mode() & 0o777 != 0o600
        {
            return Err("Auto-reset journal ownership or permissions are unsafe".into());
        }
        let mut file = OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
            .map_err(|error| format!("Unable to open auto-reset journal: {error}"))?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|error| format!("Unable to read auto-reset journal: {error}"))?;
        let journal: ResetJournal = serde_json::from_str(&content).map_err(|_| {
            "Auto-reset journal is invalid; refusing to spend a reset credit".to_string()
        })?;
        if journal.version != JOURNAL_VERSION {
            return Err(
                "Auto-reset journal version is unsupported; refusing to spend a reset credit"
                    .into(),
            );
        }
        if matches!(journal.state.as_str(), "pending" | "unknown") {
            let account_id = journal.account_id.as_deref().unwrap_or_default();
            let episode = journal.episode_key.as_deref().unwrap_or_default();
            if account_id.is_empty()
                || !episode.starts_with(&format!("{account_id}|"))
                || journal.thread_id.as_deref().is_none_or(str::is_empty)
                || journal.idempotency_key.as_deref().is_none_or(str::is_empty)
            {
                return Err("Auto-reset journal has an incomplete uncertain attempt".into());
            }
        }
        Ok(journal)
    }

    pub(super) fn load() -> Result<ResetJournal, String> {
        Self::load_at(&Self::path())
    }

    /// This is a small, random-access state document rather than a log, so it is
    /// atomically replaced and kept uncompressed. Brotli would make each daemon
    /// tick needlessly expensive and does not support safe in-place updates.
    pub(super) fn write_at(path: &Path, journal: &ResetJournal) -> Result<(), String> {
        let parent = path
            .parent()
            .ok_or("Auto-reset journal has no parent directory")?;
        fs::create_dir_all(parent).map_err(|error| error.to_string())?;
        fs::set_permissions(parent, fs::Permissions::from_mode(0o700))
            .map_err(|error| error.to_string())?;
        let content = serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?;
        let temp = parent.join(format!(
            ".auto-reset-state.{}.{}.tmp",
            std::process::id(),
            NEXT_IDEMPOTENCY_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        ));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .mode(0o600)
                .open(&temp)
                .map_err(|error| error.to_string())?;
            file.write_all(&content)
                .map_err(|error| error.to_string())?;
            file.sync_all().map_err(|error| error.to_string())?;
            fs::rename(&temp, path).map_err(|error| error.to_string())?;
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))
                .map_err(|error| error.to_string())?;
            Ok(())
        })();
        if result.is_err() {
            let _ = fs::remove_file(&temp);
        }
        result
    }

    pub(super) fn write(journal: &ResetJournal) -> Result<(), String> {
        Self::write_at(&Self::path(), journal)
    }
}
