use std::path::PathBuf;

/// What one walk of Codex's rollout roots found.
#[derive(Debug, Default)]
pub struct RolloutScan {
    /// Plain canonical rollouts old enough to compress, oldest first.
    pub candidates: Vec<PathBuf>,
    /// Staging files left by an interrupted earlier run.
    pub stale_staging: Vec<PathBuf>,
    /// Directories or entries that could not be read.
    pub errors: u64,
}
