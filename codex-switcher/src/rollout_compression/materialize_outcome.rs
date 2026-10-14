/// What `cxi sessions decompress` did for one thread.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MaterializeOutcome {
    /// The plain rollout was restored and the compressed file removed.
    Restored { bytes: u64 },
    /// The plain rollout already exists; Codex reads it first.
    AlreadyPlain,
    /// No compressed rollout exists for the thread.
    NotCompressed,
    /// A Codex writer owns the thread, or coordination stayed busy.
    Busy,
    /// The compressed file changed while it was being decoded.
    Changed,
}
