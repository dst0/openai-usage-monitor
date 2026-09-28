use std::{
    fs::{self, OpenOptions},
    io::Read,
    os::unix::fs::{MetadataExt, OpenOptionsExt},
    path::Path,
};

pub(super) const MAX_RECOVERY_MANIFEST_BYTES: u64 = 1024 * 1024;

/// A missing manifest is allowed. An unsafe or changing path is never absence.
pub(super) fn read_manifest_bytes(path: &Path) -> Result<Option<Vec<u8>>, String> {
    let named = match fs::symlink_metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(error.to_string()),
    };
    if !named.is_file()
        || named.file_type().is_symlink()
        || named.len() > MAX_RECOVERY_MANIFEST_BYTES
    {
        return Err("Recovery manifest is unsafe or exceeds its size limit".into());
    }
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .map_err(|error| error.to_string())?;
    let opened = file.metadata().map_err(|error| error.to_string())?;
    if opened.dev() != named.dev() || opened.ino() != named.ino() || opened.len() != named.len() {
        return Err("Recovery manifest changed while opening".into());
    }
    let mut bytes = Vec::with_capacity(named.len() as usize);
    file.take(MAX_RECOVERY_MANIFEST_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| error.to_string())?;
    let after = fs::symlink_metadata(path).map_err(|error| error.to_string())?;
    if bytes.len() as u64 != named.len()
        || after.dev() != named.dev()
        || after.ino() != named.ino()
        || after.len() != named.len()
        || after.modified().ok() != named.modified().ok()
        || (after.ctime(), after.ctime_nsec()) != (named.ctime(), named.ctime_nsec())
    {
        return Err("Recovery manifest changed while reading".into());
    }
    Ok(Some(bytes))
}
