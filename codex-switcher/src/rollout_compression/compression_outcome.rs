/// What happened to one rollout during a compression pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CompressionOutcome {
    /// The `.jsonl.zst` was published and the plain file removed.
    Compressed {
        source_bytes: u64,
        compressed_bytes: u64,
    },
    /// Modified more recently than the minimum age.
    NotCold,
    /// A compressed sibling already exists; Codex reads the plain file first,
    /// so nothing is replaced.
    AlreadyCompressed,
    /// Not a regular canonical rollout file (a symlink, directory, or other
    /// name), or it disappeared before it was opened.
    NotRollout,
    /// The first record does not name the thread in the file name.
    MetadataMismatch,
    /// A Codex writer owns the thread, or writer coordination stayed busy.
    Busy,
    /// The file changed while it was being compressed; it is left as it is.
    Changed,
    /// Kept plain on purpose, for example while recovery still tracks it.
    Excluded,
}

impl CompressionOutcome {
    pub fn label(self) -> &'static str {
        match self {
            Self::Compressed { .. } => "compressed",
            Self::NotCold => "not_cold",
            Self::AlreadyCompressed => "already_compressed",
            Self::NotRollout => "not_rollout",
            Self::MetadataMismatch => "metadata_mismatch",
            Self::Busy => "busy",
            Self::Changed => "changed",
            Self::Excluded => "excluded",
        }
    }
}
