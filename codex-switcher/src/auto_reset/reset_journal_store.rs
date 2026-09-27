use super::reset_journal::{ResetJournal, JOURNAL_VERSION};
use crate::state_file::{PrivateStateFileWriteService, StateFileOperations};
use crate::storage;
use std::fs;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

const JOURNAL_FILE: &str = "auto-reset-state.json";

static NEXT_STAGING_SEQUENCE: AtomicU64 = AtomicU64::new(1);

/// The single automatic reset journal in `CODEX_HOME`.
pub(super) struct ResetJournalStore<'a> {
    directory: PathBuf,
    files: &'a dyn StateFileOperations,
}

impl<'a> ResetJournalStore<'a> {
    pub(super) fn new(files: &'a dyn StateFileOperations) -> Self {
        Self::in_directory(storage::codex_home(), files)
    }

    pub(super) fn in_directory(directory: PathBuf, files: &'a dyn StateFileOperations) -> Self {
        Self { directory, files }
    }

    pub(super) fn load(&self) -> Result<ResetJournal, String> {
        let path = self.directory.join(JOURNAL_FILE);
        let metadata = match fs::symlink_metadata(&path) {
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
        let mut file = self
            .files
            .open_for_read(&path)
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

    /// Durably replaces the journal. A directory-sync failure is reported
    /// after the new journal became visible, so a caller must not assume the
    /// previous journal is still in place when this returns an error.
    pub(super) fn write(&self, journal: &ResetJournal) -> Result<(), String> {
        let content = serde_json::to_vec_pretty(journal).map_err(|error| error.to_string())?;
        // Uninstall matches the `.auto-reset-state.` prefix and `.tmp` suffix.
        let staging = format!(
            ".auto-reset-state.{}.{}.tmp",
            std::process::id(),
            NEXT_STAGING_SEQUENCE.fetch_add(1, Ordering::Relaxed)
        );
        PrivateStateFileWriteService::new(self.files)
            .replace(&self.directory, JOURNAL_FILE, &staging, &content)
            .map_err(|failure| format!("Auto-reset journal {failure}"))
    }
}

#[cfg(test)]
#[path = "reset_journal_store.test.rs"]
mod tests;
