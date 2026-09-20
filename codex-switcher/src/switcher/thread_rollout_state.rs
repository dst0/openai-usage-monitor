#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadRolloutState {
    /// Turn completed cleanly with task_complete and no error. Never resume.
    CleanCompleted,
    /// Turn ended with an error indicating usage/rate limits or credit exhaustion.
    InterruptedByQuota,
    /// Turn was aborted/interrupted by user or cancelled.
    TurnAborted,
    /// Turn was actively executing mid-flight (user message, tool call, reasoning in flight).
    ActiveInProgress,
    /// No significant events or unparseable.
    Unknown,
}
