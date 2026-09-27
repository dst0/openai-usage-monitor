use super::state_file_operations::StateFileOperations;
use std::fs::{self, File, OpenOptions};
use std::io::{self, Write};
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
use std::path::Path;

/// The real filesystem.
pub(crate) struct SystemStateFileOperations;

impl StateFileOperations for SystemStateFileOperations {
    fn prepare_directory(&self, directory: &Path) -> io::Result<()> {
        fs::create_dir_all(directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o700))
    }

    fn create_staging(&self, staging: &Path) -> io::Result<File> {
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .custom_flags(libc::O_NOFOLLOW)
            .mode(0o600)
            .open(staging)
    }

    fn save_staging(&self, file: &mut File, content: &[u8]) -> io::Result<()> {
        file.write_all(content)?;
        // The creation mode is filtered by the umask; readers require exactly
        // 0600, so set it on the descriptor before the file becomes visible.
        file.set_permissions(fs::Permissions::from_mode(0o600))?;
        file.sync_all()
    }

    fn replace(&self, staging: &Path, path: &Path) -> io::Result<()> {
        fs::rename(staging, path)
    }

    fn sync_directory(&self, directory: &Path) -> io::Result<()> {
        File::open(directory)?.sync_all()
    }

    fn remove_staging(&self, staging: &Path) -> io::Result<()> {
        fs::remove_file(staging)
    }

    fn open_for_read(&self, path: &Path) -> io::Result<File> {
        OpenOptions::new()
            .read(true)
            .custom_flags(libc::O_NOFOLLOW)
            .open(path)
    }
}
