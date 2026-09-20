#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RestoreOutcome {
    InProgress,
    Restored,
    Partial,
    Failed,
}
