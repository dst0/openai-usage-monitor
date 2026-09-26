#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThreadRolloutState {
    /// Turn completed cleanly with task_complete and no error. Never resume.
    CleanCompleted,
    /// Turn ended with an error indicating usage/rate limits or credit exhaustion.
    InterruptedByQuota,
    /// Turn ended with a non-quota error (for example an auth or transport
    /// outage) before producing a final agent message. The work is unfinished,
    /// but the error may be a policy block, so only an explicit resume continues it.
    InterruptedByError,
    /// Turn was aborted/interrupted by user or cancelled.
    TurnAborted,
    /// Turn was actively executing mid-flight (user message, tool call, reasoning in flight).
    ActiveInProgress,
    /// No significant events or unparseable.
    Unknown,
}
