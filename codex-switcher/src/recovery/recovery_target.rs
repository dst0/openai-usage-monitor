use super::{
    observer::Observer,
    queue_snapshot::{pending_count, query},
    target_dispatch::writer_is_locked,
    thread_identity::valid_id,
};
use crate::switcher;
use std::{
    os::unix::fs::MetadataExt,
    path::Path,
    time::{Duration, Instant},
};
pub(super) const RECOVERY_DISPATCH_TIMEOUT: Duration = Duration::from_secs(90);
pub(super) const RECOVERY_EXECUTION_TIMEOUT: Duration = Duration::from_secs(600);
pub(crate) const RECOVERY_SOAK_WINDOW: Duration = Duration::from_secs(10);
pub(super) const FOREGROUND_SCAN_BUDGET_BYTES: u64 = 16 * 1024 * 1024;

pub(super) struct RecoveryTarget {
    pub(super) id: String,
    pub(super) state: switcher::ThreadRolloutState,
    pub(super) writer_locked: bool,
    pub(super) observer: Observer,
    pub(super) scan_complete: bool,
    pub(super) existing_queue: usize,
    pub(super) mounted_by_recovery: bool,
    pub(super) owner_unavailable: bool,
    pub(super) account_mismatch: bool,
    pub(super) dispatched: bool,
    pub(super) completed: bool,
    pub(super) failure: Option<String>,
    pub(super) deadline: Instant,
    pub(super) execution_deadline_set: bool,
    pub(super) expected_turn_id: Option<String>,
    pub(super) proof_observed_at: Option<Instant>,
}

pub(super) fn prepare_target(
    home: &Path,
    id: &str,
    baseline: Option<u64>,
    scan_budget: u64,
) -> Result<Option<RecoveryTarget>, String> {
    use switcher::ThreadRolloutState;
    if !valid_id(id) {
        return Err("Invalid thread ID".into());
    }
    let stored_id = query(&home.join("state_5.sqlite"), &format!(
        "SELECT id FROM threads WHERE id = '{id}' AND archived = 0 AND (thread_source IS NULL OR thread_source != 'subagent');"))?;
    if stored_id != id {
        return Err("Target is absent, archived, or a subagent; refusing recovery".into());
    }
    let state = switcher::inspect_thread_rollout_state(home, id);
    let pending = pending_count(home, id)?;
    if state == ThreadRolloutState::CleanCompleted && baseline.is_none() && pending == 0 {
        crate::runtime_print!("RECOVERY_SKIPPED thread={id} reason=completed");
        return Ok(None);
    }
    if state == ThreadRolloutState::Unknown && baseline.is_none() {
        return Err("Unknown rollout state; refusing automatic recovery".into());
    }
    let path = switcher::find_thread_rollout_path(home, id).ok_or("Missing target rollout")?;
    let queued_once = pending > 0;
    let mut target = RecoveryTarget {
        id: id.to_string(),
        state,
        writer_locked: writer_is_locked(home, id),
        observer: match baseline {
            // A deferred append cursor samples only the checkpoint boundary.
            // Foreground dispatch must re-read the full checkpoint interval:
            // a same-inode middle rewrite can preserve both samples.
            Some(offset) => Observer::checkpoint_at(path, offset)?,
            None => Observer::checkpoint(path)?,
        },
        scan_complete: false,
        existing_queue: pending,
        mounted_by_recovery: false,
        owner_unavailable: false,
        account_mismatch: false,
        dispatched: false,
        completed: false,
        failure: None,
        deadline: Instant::now() + RECOVERY_DISPATCH_TIMEOUT,
        execution_deadline_set: false,
        expected_turn_id: None,
        proof_observed_at: None,
    };
    record_target_state_with_budget(&mut target, scan_budget)?;
    // A target left in an older restart manifest may have been completed
    // manually since that failed run. If no new post-checkpoint work belongs to
    // this operation, completion is terminal and must not be revived or waited
    // on for three minutes.
    if target.scan_complete
        && target.state == ThreadRolloutState::CleanCompleted
        && !target.completed
        && !queued_once
    {
        crate::runtime_print!("RECOVERY_SKIPPED thread={id} reason=completed");
        return Ok(None);
    }
    Ok(Some(target))
}

#[cfg(test)]
pub(super) fn record_target_state_at(
    target: &mut RecoveryTarget,
    now: Instant,
) -> Result<(), String> {
    record_target_state_with_budget_at(target, now, FOREGROUND_SCAN_BUDGET_BYTES)
}

fn record_target_state_with_budget_at(
    target: &mut RecoveryTarget,
    now: Instant,
    scan_budget: u64,
) -> Result<(), String> {
    let before = target
        .observer
        .path
        .metadata()
        .map_err(|error| error.to_string())?;
    if !target.dispatched && !target.observer.pre_dispatch_snapshot_matches(&before) {
        return Err("Rollout identity changed before recovery dispatch".into());
    }
    let snapshot_end = before.len();
    let limit = snapshot_end.min(target.observer.offset.saturating_add(scan_budget));
    target.observer.poll_to(limit)?;
    if target.observer.saw_oversized || target.observer.saw_malformed {
        return Err("Unreadable rollout record prevents recovery verification".into());
    }
    let after = target
        .observer
        .path
        .metadata()
        .map_err(|error| error.to_string())?;
    if before.dev() != after.dev()
        || before.ino() != after.ino()
        || after.len() < before.len()
        || (!target.dispatched && after.len() != before.len())
        || (after.len() == before.len()
            && (after.modified().ok() != before.modified().ok()
                || (after.ctime(), after.ctime_nsec()) != (before.ctime(), before.ctime_nsec())))
    {
        return Err("Rollout changed while recovery was checking its checkpoint".into());
    }
    target.scan_complete =
        limit == after.len() && target.observer.partial.is_empty() && !target.observer.oversized;
    if !target.scan_complete {
        return Ok(());
    }
    if target.dispatched
        && target.existing_queue > 0
        && target.expected_turn_id.is_none()
        && target.observer.evidence.started
    {
        target.expected_turn_id = target.observer.evidence.start_turn_id.clone();
        if let Some(turn_id) = target.expected_turn_id.as_deref() {
            crate::runtime_print!(
                "RECOVERY_QUEUE_TURN_BOUND thread={} turn={} source=post_ack_rollout",
                target.id,
                turn_id
            );
        }
    }
    if let Some(expected) = target.expected_turn_id.as_deref() {
        if target.observer.evidence.started
            && target.observer.evidence.start_turn_id.as_deref() != Some(expected)
        {
            target.failure = Some(format!(
                "Desktop started unexpected turn {} instead of IPC-confirmed turn {expected}",
                target
                    .observer
                    .evidence
                    .start_turn_id
                    .as_deref()
                    .unwrap_or("unknown")
            ));
            return Ok(());
        }
    }
    let started = target
        .observer
        .evidence
        .matches_expected_turn(target.expected_turn_id.as_deref());
    if started && !target.execution_deadline_set {
        // task_started proves that the app-server accepted dispatch, but long
        // threads can spend several minutes compacting before the first model
        // item. Keep waiting for substantive work without misreporting the
        // intermediate active state as successful recovery.
        target.deadline = Instant::now() + RECOVERY_EXECUTION_TIMEOUT;
        target.execution_deadline_set = true;
        crate::runtime_print!(
            "RECOVERY_STARTED thread={} verification_timeout_secs={}",
            target.id,
            RECOVERY_EXECUTION_TIMEOUT.as_secs()
        );
    }
    if target.observer.evidence.failed {
        target.failure = Some(if target.observer.evidence.aborted {
            "Target was interrupted during recovery".into()
        } else {
            "Target reported an error during recovery".into()
        });
        return Ok(());
    }
    if target
        .observer
        .evidence
        .verified(target.expected_turn_id.as_deref())
    {
        let observed_at = *target.proof_observed_at.get_or_insert_with(|| {
            crate::runtime_print!(
                "RECOVERY_WORK_OBSERVED thread={} start={} work={} soak_secs={}",
                target.id,
                target
                    .observer
                    .evidence
                    .start_time
                    .as_deref()
                    .unwrap_or("existing-turn"),
                target
                    .observer
                    .evidence
                    .work_time
                    .as_deref()
                    .unwrap_or("observed"),
                RECOVERY_SOAK_WINDOW.as_secs()
            );
            now
        });
        if proof_survived_stability_window(observed_at, now) {
            if !target.completed {
                let msg = format!(
                    "RECOVERY_VERIFIED thread={} turn={} stable_secs={}",
                    target.id,
                    target
                        .expected_turn_id
                        .as_deref()
                        .unwrap_or("desktop-native"),
                    RECOVERY_SOAK_WINDOW.as_secs()
                );
                crate::runtime_print!("{msg}");
                crate::logger::log("INFO", "RECOVERY", &msg);
            }
            target.completed = true;
        }
    } else {
        target.proof_observed_at = None;
    }
    Ok(())
}

pub(super) fn record_target_state_with_budget(
    target: &mut RecoveryTarget,
    scan_budget: u64,
) -> Result<(), String> {
    record_target_state_with_budget_at(target, Instant::now(), scan_budget)
}

pub(super) fn proof_survived_stability_window(observed_at: Instant, now: Instant) -> bool {
    now.duration_since(observed_at) >= RECOVERY_SOAK_WINDOW
}
