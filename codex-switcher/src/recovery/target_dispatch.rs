use super::{
    desktop_ipc::DesktopIpc,
    queue_snapshot::{
        prepare_interrupted_queue, queue_revision, queued_messages,
        validate_queue_snapshot_revision,
    },
    recovery_mode::RecoveryMode,
    recovery_target::{RecoveryTarget, RECOVERY_DISPATCH_TIMEOUT},
};
use crate::switcher;
use fs2::FileExt;
use std::{fs::File, path::Path, time::Instant};

pub(super) fn should_dispatch(
    state: switcher::ThreadRolloutState,
    pending: usize,
    mode: RecoveryMode,
) -> bool {
    use switcher::ThreadRolloutState::*;
    // A writer lock proves only that Desktop has mounted/owns the thread. It
    // does not distinguish a running turn from an interrupted one. Therefore
    // ambiguous ActiveInProgress is dispatchable only when this operation owns
    // a pre-restart checkpoint or the user explicitly named the target.
    pending == 0
        && (matches!(state, InterruptedByQuota | TurnAborted)
            || (state == ActiveInProgress && mode.allows_ambiguous_active_dispatch()))
}

pub(super) fn writer_is_locked(home: &Path, id: &str) -> bool {
    let path = home.join("thread-writer-locks").join(format!("{id}.lock"));
    match File::open(path) {
        Ok(file) => file.try_lock_exclusive().is_err(),
        Err(e) => e.kind() != std::io::ErrorKind::NotFound,
    }
}

pub(super) fn dispatch_if_needed(
    home: &Path,
    desktop: &mut DesktopIpc,
    target: &mut RecoveryTarget,
    mode: RecoveryMode,
) -> Result<(), String> {
    if target.completed || target.failure.is_some() || target.observer.evidence.started {
        return Ok(());
    }
    let queue_revision_before = queue_revision(home, &target.id)?;
    let mut messages = queued_messages(home, &target.id)?;
    let queue_revision_after = queue_revision(home, &target.id)?;
    validate_queue_snapshot_revision(queue_revision_before, queue_revision_after)?;
    let pending = messages.len();
    if pending > 0 {
        target.existing_queue = pending;
        let unpaused = prepare_interrupted_queue(&mut messages)?;
        // Mark before IPC. A disconnect after forwarding has an unknown
        // outcome, so this operation must not retry the queue update.
        target.dispatched = true;
        target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
        if unpaused {
            target.mounted_by_recovery = desktop.resume_existing_queue(&target.id, messages)?;
            crate::runtime_print!(
                "RECOVERY_QUEUE_UNPAUSED thread={} messages={} transport=desktop_ipc",
                target.id,
                pending
            );
        } else {
            // Mounting an already-unpaused queue wakes the owner's coordinator.
            let (_, mounted_by_recovery) = desktop.ensure_thread_owner(&target.id)?;
            target.mounted_by_recovery = mounted_by_recovery;
            crate::runtime_print!(
                "RECOVERY_QUEUE_MOUNTED thread={} messages={} transport=desktop_ipc",
                target.id,
                pending
            );
        }
        return Ok(());
    }
    target.writer_locked = writer_is_locked(home, &target.id);
    if !should_dispatch(target.state, pending, mode) {
        return Err(format!(
            "Thread state {:?} is ambiguous without a pre-restart checkpoint; refusing to touch a possibly manually resumed task",
            target.state,
        ));
    }
    // Mark before the call. A timeout is an unknown outcome, so this operation
    // must never retry and risk starting the interrupted turn twice.
    target.dispatched = true;
    target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
    let (mounted_by_recovery, turn_id) = desktop.resume_interrupted_turn(&target.id)?;
    target.mounted_by_recovery = mounted_by_recovery;
    target.expected_turn_id = Some(turn_id.clone());
    crate::runtime_print!(
        "RECOVERY_DISPATCHED thread={} turn={} transport=desktop_ipc trigger=app_update_resume",
        target.id,
        turn_id
    );
    Ok(())
}
