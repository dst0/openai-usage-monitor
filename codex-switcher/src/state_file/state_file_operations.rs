use std::fs::File;
use std::io;
use std::path::Path;

/// Filesystem calls used to read and durably replace a small private state
/// document. Production uses `SystemStateFileOperations`; tests inject a fake
/// that fails a chosen call, so each failure point is proven deterministically.
pub(crate) trait StateFileOperations {
    /// Creates `directory` if needed and restricts it to the owner.
    fn prepare_directory(&self, directory: &Path) -> io::Result<()>;

    /// Exclusively creates a new owner-only staging file without following a
    /// symlink at that name.
    fn create_staging(&self, staging: &Path) -> io::Result<File>;

    /// Writes the complete content, forces owner-only mode, and flushes the
    /// file to stable storage.
    fn save_staging(&self, file: &mut File, content: &[u8]) -> io::Result<()>;

    /// Atomically renames the staging file over the final name.
    fn replace(&self, staging: &Path, path: &Path) -> io::Result<()>;

    /// Flushes `directory` so a completed rename survives power loss.
    fn sync_directory(&self, directory: &Path) -> io::Result<()>;

    /// Removes an abandoned staging file.
    fn remove_staging(&self, staging: &Path) -> io::Result<()>;

    /// Opens a state file for reading without following a final symlink.
    fn open_for_read(&self, path: &Path) -> io::Result<File>;
}
