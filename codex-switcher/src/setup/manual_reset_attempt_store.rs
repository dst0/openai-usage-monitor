use super::manual_reset_attempt::ManualResetAttempt;
use crate::state_file::{PrivateStateFileWriteService, StateFileOperations, StateFileWriteFailure};
use crate::storage::codex_home;
use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::path::PathBuf;

const JOURNAL_FILE: &str = "manual-reset-state.json";
const MAX_JOURNAL_BYTES: u64 = 16 * 1024;

/// The single private manual reset attempt in `CODEX_HOME`.
pub(super) struct ManualResetAttemptStore<'a> {
    files: &'a dyn StateFileOperations,
}

impl<'a> ManualResetAttemptStore<'a> {
    pub(super) fn new(files: &'a dyn StateFileOperations) -> Self {
        Self { files }
    }

    pub(super) fn path() -> PathBuf {
        codex_home().join(JOURNAL_FILE)
    }

    pub(super) fn load(&self) -> Result<Option<ManualResetAttempt>, String> {
        let mut file = match self.files.open_for_read(&Self::path()) {
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

    /// Durably replaces the attempt. A `SyncDirectory` failure is reported
    /// after the new record became visible, so a caller must not assume the
    /// previous record is still in place when this returns an error.
    pub(super) fn write(&self, attempt: &ManualResetAttempt) -> Result<(), String> {
        attempt.validate()?;
        let content = serde_json::to_vec(attempt)
            .map_err(|_| "Manual reset attempt could not be encoded".to_string())?;
        let mut nonce = [0u8; 8];
        getrandom::getrandom(&mut nonce)
            .map_err(|_| "Manual reset attempt nonce unavailable".to_string())?;
        // Uninstall matches this exact interrupted-staging name.
        let staging = format!(
            "manual-reset-state.{}.{:016x}.tmp.json",
            std::process::id(),
            u64::from_ne_bytes(nonce)
        );
        PrivateStateFileWriteService::new(self.files)
            .replace(&codex_home(), JOURNAL_FILE, &staging, &content)
            .map_err(|failure| {
                match failure {
                    StateFileWriteFailure::PrepareDirectory(_) => {
                        "Manual reset directory could not be prepared"
                    }
                    StateFileWriteFailure::CreateStaging(_) => {
                        "Manual reset staging file could not be created"
                    }
                    StateFileWriteFailure::SaveStaging(_) => {
                        "Manual reset staging file could not be saved"
                    }
                    StateFileWriteFailure::Replace(_) => {
                        "Manual reset attempt could not be replaced"
                    }
                    StateFileWriteFailure::SyncDirectory(_) => {
                        "Manual reset attempt directory sync failed"
                    }
                }
                .to_string()
            })
    }
}

#[cfg(test)]
#[path = "manual_reset_attempt_store.test.rs"]
mod tests;
