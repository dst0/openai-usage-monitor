use std::path::{Path, PathBuf};

/// Suffix Codex appends to a rollout's plain name for its compressed form.
pub const COMPRESSED_SUFFIX: &str = ".zst";
const PLAIN_SUFFIX: &str = ".jsonl";
const PREFIX: &str = "rollout-";
/// `YYYY-MM-DDThh-mm-ss`, as Codex renders rollout timestamps.
const TIMESTAMP_LEN: usize = 19;

/// A canonical plain rollout basename:
/// `rollout-<timestamp>-<thread id>[_<rollout id>].jsonl`.
///
/// Reverted threads append a second ID after an underscore; the first one is
/// always the thread ID, which keys the thread's writer lock.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RolloutFileName {
    thread_id: String,
}

impl RolloutFileName {
    /// Parses a plain rollout basename. Compressed names, temporary files and
    /// anything else return `None`.
    pub fn parse(name: &str) -> Option<Self> {
        let core = name.strip_prefix(PREFIX)?.strip_suffix(PLAIN_SUFFIX)?;
        let timestamp = core.get(..TIMESTAMP_LEN)?;
        if !Self::is_timestamp(timestamp) || core.get(TIMESTAMP_LEN..TIMESTAMP_LEN + 1)? != "-" {
            return None;
        }
        let ids = core.get(TIMESTAMP_LEN + 1..)?;
        let (thread_id, rollout_id) = ids.split_once('_').unwrap_or((ids, ids));
        if !crate::switcher::is_valid_thread_id(thread_id)
            || !crate::switcher::is_valid_thread_id(rollout_id)
        {
            return None;
        }
        Some(Self {
            thread_id: thread_id.to_ascii_lowercase(),
        })
    }

    /// Parses the basename of `path`.
    pub fn from_path(path: &Path) -> Option<Self> {
        Self::parse(path.file_name()?.to_str()?)
    }

    pub fn thread_id(&self) -> &str {
        &self.thread_id
    }

    /// The compressed sibling Codex looks for when the plain file is absent.
    pub fn compressed_path(plain: &Path) -> PathBuf {
        let mut name = plain.as_os_str().to_os_string();
        name.push(COMPRESSED_SUFFIX);
        PathBuf::from(name)
    }

    /// The plain path for a compressed sibling, or `None` for other names.
    pub fn plain_path(compressed: &Path) -> Option<PathBuf> {
        let text = compressed.to_str()?;
        let plain = text.strip_suffix(COMPRESSED_SUFFIX)?;
        Self::from_path(Path::new(plain))?;
        Some(PathBuf::from(plain))
    }

    fn is_timestamp(text: &str) -> bool {
        text.bytes().enumerate().all(|(index, byte)| match index {
            4 | 7 | 13 | 16 => byte == b'-',
            10 => byte == b'T',
            _ => byte.is_ascii_digit(),
        })
    }
}

#[cfg(test)]
#[path = "rollout_file_name.test.rs"]
mod tests;
