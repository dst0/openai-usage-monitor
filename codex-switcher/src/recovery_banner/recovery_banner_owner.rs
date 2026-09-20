use fs2::FileExt;
use std::fs::{self, File, OpenOptions};
use std::os::unix::fs::{MetadataExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};

/// A same-user non-blocking lease used to guarantee one banner owner.
pub struct RecoveryBannerOwner {
    file: File,
    path: PathBuf,
}

impl RecoveryBannerOwner {
    pub fn acquire(path: impl AsRef<Path>) -> Result<Self, String> {
        let path = path.as_ref().to_path_buf();
        let parent = path
            .parent()
            .ok_or_else(|| "Banner owner path has no parent".to_string())?;
        ensure_private_directory(parent)?;
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)
            .map_err(|error| format!("Could not open banner owner lease: {error}"))?;
        let metadata = file
            .metadata()
            .map_err(|error| format!("Could not inspect banner owner lease: {error}"))?;
        let expected_uid = unsafe { libc::geteuid() };
        if !metadata.is_file() || metadata.uid() != expected_uid || metadata.mode() & 0o777 != 0o600
        {
            return Err("Banner owner lease has unsafe ownership or permissions".into());
        }
        file.try_lock_exclusive()
            .map_err(|_| "Another recovery banner already owns the display".to_string())?;
        Ok(Self { file, path })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for RecoveryBannerOwner {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn ensure_private_directory(path: &Path) -> Result<(), String> {
    match fs::symlink_metadata(path) {
        Ok(metadata) if metadata.file_type().is_symlink() => {
            return Err("Banner owner directory must not be a symlink".into());
        }
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            fs::create_dir_all(path).map_err(|error| error.to_string())?;
        }
        Err(error) => return Err(error.to_string()),
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
        .map_err(|error| error.to_string())?;
    let metadata = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    let expected_uid = unsafe { libc::geteuid() };
    if !metadata.is_dir()
        || metadata.file_type().is_symlink()
        || metadata.uid() != expected_uid
        || metadata.mode() & 0o077 != 0
    {
        return Err("Banner owner directory is not private to the current user".into());
    }
    Ok(())
}
