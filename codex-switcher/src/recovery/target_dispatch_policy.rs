use super::super::recovery_mode::RecoveryMode;
use crate::switcher::ThreadRolloutState;

pub(in crate::recovery) fn should_dispatch(
    state: ThreadRolloutState,
    pending: usize,
    mode: RecoveryMode,
) -> bool {
    use ThreadRolloutState::*;
    // A writer lock proves only that Desktop has mounted/owns the thread. It
    // does not distinguish a running turn from an interrupted one. Therefore
    // ambiguous ActiveInProgress is dispatchable only when this operation owns
    // a pre-restart checkpoint or the user explicitly named the target.
    pending == 0
        && match state {
            InterruptedByQuota => true,
            InterruptedByError => mode == RecoveryMode::ExplicitTarget,
            TurnAborted | ActiveInProgress => mode.allows_ambiguous_active_dispatch(),
            _ => false,
        }
}

pub(in crate::recovery) fn should_resume_queued(
    state: ThreadRolloutState,
    mode: RecoveryMode,
) -> bool {
    use ThreadRolloutState::*;
    match state {
        InterruptedByQuota => true,
        InterruptedByError => mode == RecoveryMode::ExplicitTarget,
        TurnAborted | ActiveInProgress => mode.allows_ambiguous_active_dispatch(),
        // A captured or explicitly selected follow-up can remain queued after
        // its preceding turn completed. Discovery alone cannot claim it.
        CleanCompleted => mode != RecoveryMode::DiscoveredOnly,
        _ => false,
    }
}
