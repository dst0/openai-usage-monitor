use super::state_file_operations::StateFileOperations;
use super::state_file_write_failure::StateFileWriteFailure;
use std::path::Path;

/// Atomically and durably replaces one small private state document.
///
/// Reset journals are random-access state rather than logs, so they are
/// replaced whole and kept uncompressed; Brotli would add work to every write
/// and does not support safe in-place updates.
pub(crate) struct PrivateStateFileWriteService<'a> {
    operations: &'a dyn StateFileOperations,
}

impl<'a> PrivateStateFileWriteService<'a> {
    pub(crate) fn new(operations: &'a dyn StateFileOperations) -> Self {
        Self { operations }
    }

    /// Stages `content` under `staging_name` in `directory`, flushes it,
    /// renames it over `file_name`, then flushes the directory. A failure
    /// before the rename removes the staging file and leaves the previous
    /// document unchanged; a directory-sync failure leaves the new content
    /// visible but possibly not durable, and callers must treat it as such.
    pub(crate) fn replace(
        &self,
        directory: &Path,
        file_name: &str,
        staging_name: &str,
        content: &[u8],
    ) -> Result<(), StateFileWriteFailure> {
        let path = directory.join(file_name);
        let staging = directory.join(staging_name);
        self.operations
            .prepare_directory(directory)
            .map_err(StateFileWriteFailure::PrepareDirectory)?;
        let mut file = self
            .operations
            .create_staging(&staging)
            .map_err(StateFileWriteFailure::CreateStaging)?;
        let staged = self
            .operations
            .save_staging(&mut file, content)
            .map_err(StateFileWriteFailure::SaveStaging);
        drop(file);
        let replaced = staged.and_then(|()| {
            self.operations
                .replace(&staging, &path)
                .map_err(StateFileWriteFailure::Replace)
        });
        if replaced.is_err() {
            let _ = self.operations.remove_staging(&staging);
        }
        replaced?;
        // The rename is only as durable as the directory entry that names it.
        self.operations
            .sync_directory(directory)
            .map_err(StateFileWriteFailure::SyncDirectory)
    }
}

#[cfg(test)]
#[path = "private_state_file_write_service.test.rs"]
mod tests;
