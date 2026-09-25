use super::{
    desktop_ipc::DesktopIpc,
    dispatch_mark_error::DispatchMarkError,
    ipc_call_error::IpcCallError,
    manifest_store::mark_dispatch_attempt,
    queue_snapshot::{
        pending_count, prepare_interrupted_queue, queue_revision, queued_messages,
        validate_queue_snapshot_revision,
    },
    recovery_mode::RecoveryMode,
    recovery_target::{record_target_state, RecoveryTarget, RECOVERY_DISPATCH_TIMEOUT},
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

fn resolve_owner(desktop: &mut DesktopIpc, target: &mut RecoveryTarget) -> Result<String, String> {
    handle_owner_resolution(desktop.ensure_thread_owner(&target.id), target)
}

pub(super) fn handle_owner_resolution(
    result: Result<(String, bool), IpcCallError>,
    target: &mut RecoveryTarget,
) -> Result<String, String> {
    match result {
        Ok((owner, mounted)) => {
            target.mounted_by_recovery = mounted;
            Ok(owner)
        }
        Err(IpcCallError::NoClientFound) => {
            target.owner_unavailable = true;
            Err("Codex Desktop did not mount the thread within 90s (no-client-found)".into())
        }
        Err(error) => {
            // Every error here precedes dispatch. Preserve the original
            // checkpoint so a transient URL or IPC failure cannot lose work.
            target.owner_unavailable = true;
            Err(error.to_string())
        }
    }
}

fn mark_target_dispatch(target: &mut RecoveryTarget) -> Result<(), String> {
    match mark_dispatch_attempt(&target.id) {
        Ok(()) => Ok(()),
        Err(DispatchMarkError::AccountChanged) => {
            target.account_mismatch = true;
            Err(DispatchMarkError::AccountChanged.to_string())
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(super) fn revalidate_after_owner(
    home: &Path,
    target: &mut RecoveryTarget,
    prior_revision: u64,
    prior_pending: usize,
    mode: RecoveryMode,
) -> Result<bool, String> {
    validate_queue_snapshot_revision(prior_revision, queue_revision(home, &target.id)?)?;
    if pending_count(home, &target.id)? != prior_pending {
        return Err("Codex queue changed while Desktop was mounting the task".into());
    }
    record_target_state(target)?;
    if target.completed || target.failure.is_some() || target.observer.evidence.started {
        return Ok(false);
    }
    if prior_pending == 0 {
        let current_state = switcher::inspect_thread_rollout_state(home, &target.id);
        if !should_dispatch(current_state, 0, mode) {
            return Err(
                "Thread state changed while Desktop was mounting it; refusing recovery".into(),
            );
        }
    }
    Ok(true)
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
        let owner = resolve_owner(desktop, target)?;
        // Owner discovery can take 90 seconds. Never replace a queue snapshot
        // that changed while Desktop or the user was mounting the task.
        if !revalidate_after_owner(home, target, queue_revision_after, pending, mode)? {
            return Ok(());
        }
        mark_target_dispatch(target)?;
        // Mark before IPC. A disconnect after forwarding has an unknown
        // outcome, so this operation must not retry the queue update.
        target.dispatched = true;
        target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
        if unpaused {
            desktop.resume_existing_queue(&target.id, messages, &owner)?;
            crate::runtime_print!(
                "RECOVERY_QUEUE_UNPAUSED thread={} messages={} transport=desktop_ipc",
                target.id,
                pending
            );
        } else {
            // Mounting an already-unpaused queue wakes the owner's coordinator.
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
    let owner = resolve_owner(desktop, target)?;
    if !revalidate_after_owner(home, target, queue_revision_after, 0, mode)? {
        return Ok(());
    }
    mark_target_dispatch(target)?;
    // Mark before the call. A timeout is an unknown outcome, so this operation
    // must never retry and risk starting the interrupted turn twice.
    target.dispatched = true;
    target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
    let turn_id = desktop.resume_interrupted_turn(&target.id, &owner)?;
    target.expected_turn_id = Some(turn_id.clone());
    crate::runtime_print!(
        "RECOVERY_DISPATCHED thread={} turn={} transport=desktop_ipc trigger=app_update_resume",
        target.id,
        turn_id
    );
    Ok(())
}
