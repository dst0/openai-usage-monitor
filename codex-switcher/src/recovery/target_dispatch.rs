#[cfg(test)]
use super::recovery_target::FOREGROUND_SCAN_BUDGET_BYTES;
use super::{
    desktop_ipc::DesktopIpc,
    dispatch_identity_checks::DispatchIdentityChecks,
    ipc_call_error::IpcCallError,
    queue_snapshot::{
        pending_count, prepare_interrupted_queue, queue_revision, queued_messages,
        validate_queue_snapshot_revision,
    },
    recovery_dispatch_checkpoint_service::RecoveryDispatchCheckpointService,
    recovery_mode::RecoveryMode,
    recovery_target::{record_target_state_with_budget, RecoveryTarget, RECOVERY_DISPATCH_TIMEOUT},
};
use crate::switcher;
use fs2::FileExt;
use std::{fs::File, path::Path, time::Instant};

#[path = "target_dispatch_policy.rs"]
mod target_dispatch_policy;
pub(super) use target_dispatch_policy::{should_dispatch, should_resume_queued};

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

#[cfg(test)]
pub(super) fn revalidate_after_owner(
    home: &Path,
    target: &mut RecoveryTarget,
    prior_revision: u64,
    prior_pending: usize,
    mode: RecoveryMode,
) -> Result<bool, String> {
    let mut budget = FOREGROUND_SCAN_BUDGET_BYTES;
    revalidate_after_owner_with_budget(
        home,
        target,
        prior_revision,
        prior_pending,
        mode,
        &mut budget,
    )
}

pub(super) fn revalidate_after_owner_with_budget(
    home: &Path,
    target: &mut RecoveryTarget,
    prior_revision: u64,
    prior_pending: usize,
    mode: RecoveryMode,
    scan_budget: &mut u64,
) -> Result<bool, String> {
    validate_queue_snapshot_revision(prior_revision, queue_revision(home, &target.id)?)?;
    if pending_count(home, &target.id)? != prior_pending {
        return Err("Codex queue changed while Desktop was mounting the task".into());
    }
    let offset_before = target.observer.offset;
    record_target_state_with_budget(target, *scan_budget)?;
    *scan_budget = scan_budget.saturating_sub(target.observer.offset - offset_before);
    if !target.scan_complete {
        return Err("Rollout checkpoint is still being checked; refusing IPC dispatch".into());
    }
    if target.completed || target.failure.is_some() || target.observer.evidence.started {
        return Ok(false);
    }
    let current_state = switcher::inspect_thread_rollout_state(home, &target.id);
    let still_eligible = if prior_pending == 0 {
        should_dispatch(current_state, 0, mode)
    } else {
        should_resume_queued(current_state, mode)
    };
    if !still_eligible {
        return Err("Thread state changed while Desktop was mounting it; refusing recovery".into());
    }
    Ok(true)
}

#[cfg(test)]
pub(super) fn revalidate_after_banner_gate(
    home: &Path,
    target: &mut RecoveryTarget,
    prior_revision: u64,
    prior_pending: usize,
    mode: RecoveryMode,
    before_send: impl FnMut() -> Result<(), String>,
) -> Result<bool, String> {
    let mut budget = FOREGROUND_SCAN_BUDGET_BYTES;
    revalidate_after_banner_gate_with_budget(
        home,
        target,
        prior_revision,
        prior_pending,
        mode,
        &mut budget,
        before_send,
    )
}

fn revalidate_after_banner_gate_with_budget(
    home: &Path,
    target: &mut RecoveryTarget,
    prior_revision: u64,
    prior_pending: usize,
    mode: RecoveryMode,
    scan_budget: &mut u64,
    mut before_send: impl FnMut() -> Result<(), String>,
) -> Result<bool, String> {
    before_send()?;
    // Panel startup can take five seconds. Recheck the same queue and rollout
    // snapshot immediately before the durable marker and Desktop IPC request.
    if !revalidate_after_owner_with_budget(
        home,
        target,
        prior_revision,
        prior_pending,
        mode,
        scan_budget,
    )? {
        return Ok(false);
    }
    // SQLite busy retries can also outlive the panel or Desktop process. This
    // second callback can itself take time, so it cannot be the final state
    // check before the irreversible dispatch marker.
    before_send()?;
    revalidate_after_owner_with_budget(
        home,
        target,
        prior_revision,
        prior_pending,
        mode,
        scan_budget,
    )
}

pub(super) fn dispatch_if_needed(
    home: &Path,
    desktop: &mut DesktopIpc,
    target: &mut RecoveryTarget,
    mode: RecoveryMode,
    scan_budget: &mut u64,
    mut before_send: impl FnMut() -> Result<(), String>,
    identity: &mut DispatchIdentityChecks<'_>,
) -> Result<(), String> {
    if target.completed || target.failure.is_some() || target.observer.evidence.started {
        return Ok(());
    }
    if !target.scan_complete {
        return Err("Rollout checkpoint is still being checked; refusing IPC dispatch".into());
    }
    let queue_revision_before = queue_revision(home, &target.id)?;
    let mut messages = queued_messages(home, &target.id)?;
    let queue_revision_after = queue_revision(home, &target.id)?;
    validate_queue_snapshot_revision(queue_revision_before, queue_revision_after)?;
    let pending = messages.len();
    if pending > 0 {
        if !should_resume_queued(target.state, mode) {
            return Err("Thread state is not eligible for queued recovery in this mode".into());
        }
        target.existing_queue = pending;
        let unpaused = prepare_interrupted_queue(&mut messages)?;
        let owner = resolve_owner(desktop, target)?;
        // Owner discovery can take 90 seconds. Never replace a queue snapshot
        // that changed while Desktop or the user was mounting the task.
        if !revalidate_after_owner_with_budget(
            home,
            target,
            queue_revision_after,
            pending,
            mode,
            scan_budget,
        )? {
            return Ok(());
        }
        if !revalidate_after_banner_gate_with_budget(
            home,
            target,
            queue_revision_after,
            pending,
            mode,
            scan_budget,
            &mut before_send,
        )? {
            return Ok(());
        }
        RecoveryDispatchCheckpointService::mark_and_confirm(target, mode, identity)?;
        // Mark before IPC. A disconnect after forwarding has an unknown
        // outcome, so this operation must not retry the queue update.
        target.dispatched = true;
        target.deadline = Instant::now() + RECOVERY_DISPATCH_TIMEOUT;
        // Even an already-unpaused queue needs an owner-routed update to wake
        // the Desktop coordinator. Owner discovery alone is read-only.
        desktop.resume_existing_queue(&target.id, messages, &owner)?;
        if unpaused {
            crate::runtime_print!(
                "RECOVERY_QUEUE_UNPAUSED thread={} messages={} transport=desktop_ipc",
                target.id,
                pending
            );
        } else {
            crate::runtime_print!(
                "RECOVERY_QUEUE_WOKEN thread={} messages={} transport=desktop_ipc",
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
    if !revalidate_after_owner_with_budget(
        home,
        target,
        queue_revision_after,
        0,
        mode,
        scan_budget,
    )? {
        return Ok(());
    }
    if !revalidate_after_banner_gate_with_budget(
        home,
        target,
        queue_revision_after,
        0,
        mode,
        scan_budget,
        &mut before_send,
    )? {
        return Ok(());
    }
    RecoveryDispatchCheckpointService::mark_and_confirm(target, mode, identity)?;
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
