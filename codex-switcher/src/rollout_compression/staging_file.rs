use std::fs::{File, OpenOptions};
use std::io;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};

/// A private temporary file beside a rollout, removed unless published.
///
/// The name starts with a dot and never parses as a rollout, so Codex's
/// listing and its own temp cleanup ignore it; `is_stale_name` recognises
/// ours for cleanup after an interrupted run.
pub struct StagingFile {
    path: PathBuf,
    file: Option<File>,
}

const MARKER: &str = ".cxi-";
const SUFFIX: &str = ".tmp";

impl StagingFile {
    /// Creates `.<target name>.cxi-<16 hex>.tmp` exclusively, mode 0600.
    pub fn create_beside(target: &Path) -> io::Result<Self> {
        let directory = target.parent().ok_or_else(|| invalid("target has no parent"))?;
        let name = target
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| invalid("target name is not UTF-8"))?;
        let mut random = [0_u8; 8];
        getrandom::getrandom(&mut random).map_err(|err| io::Error::other(err.to_string()))?;
        let hex: String = random.iter().map(|byte| format!("{byte:02x}")).collect();
        let path = directory.join(format!(".{name}{MARKER}{hex}{SUFFIX}"));
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&path)?;
        Ok(Self {
            path,
            file: Some(file),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn file(&self) -> &File {
        self.file.as_ref().expect("staging file is open until published")
    }

    pub fn file_mut(&mut self) -> &mut File {
        self.file.as_mut().expect("staging file is open until published")
    }

    /// Links the staged file to `destination` only if nothing is there yet,
    /// then drops the staging name. `false` means `destination` existed and
    /// nothing was published.
    pub fn publish_noclobber(mut self, destination: &Path) -> io::Result<bool> {
        match std::fs::hard_link(&self.path, destination) {
            Ok(()) => {}
            Err(err) if err.kind() == io::ErrorKind::AlreadyExists => return Ok(false),
            Err(err) => return Err(err),
        }
        self.file = None;
        // The destination is already published; a leftover staging name is
        // harmless and the next run's stale-file cleanup removes it.
        let _ = std::fs::remove_file(&self.path);
        Ok(true)
    }

    /// Whether `name` is a staging name this type creates.
    pub fn is_stale_name(name: &str) -> bool {
        let Some(rest) = name.strip_prefix('.') else {
            return false;
        };
        let Some(core) = rest.strip_suffix(SUFFIX) else {
            return false;
        };
        let Some((_, hex)) = core.rsplit_once(MARKER) else {
            return false;
        };
        hex.len() == 16 && hex.bytes().all(|byte| byte.is_ascii_hexdigit())
    }
}

impl Drop for StagingFile {
    fn drop(&mut self) {
        if self.file.take().is_some() {
            let _ = std::fs::remove_file(&self.path);
        }
    }
}

fn invalid(message: &str) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidInput, message)
}
