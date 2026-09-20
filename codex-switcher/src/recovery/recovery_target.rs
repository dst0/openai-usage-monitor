use super::{
    observer::Observer,
    queue_snapshot::{pending_count, query},
    target_dispatch::writer_is_locked,
    thread_identity::valid_id,
};
use crate::switcher;
use std::{
    path::Path,
    time::{Duration, Instant},
};
pub(super) const RECOVERY_DISPATCH_TIMEOUT: Duration = Duration::from_secs(90);
pub(super) const RECOVERY_EXECUTION_TIMEOUT: Duration = Duration::from_secs(600);
pub(crate) const RECOVERY_SOAK_WINDOW: Duration = Duration::from_secs(10);

pub(super) struct RecoveryTarget {
    pub(super) id: String,
    pub(super) state: switcher::ThreadRolloutState,
    pub(super) writer_locked: bool,
    pub(super) observer: Observer,
    pub(super) existing_queue: usize,
    pub(super) mounted_by_recovery: bool,
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
            Some(offset) => Observer::checkpoint_at(path, offset)?,
            None => Observer::checkpoint(path)?,
        },
        existing_queue: pending,
        mounted_by_recovery: false,
        dispatched: false,
        completed: false,
        failure: None,
        deadline: Instant::now() + RECOVERY_DISPATCH_TIMEOUT,
        execution_deadline_set: false,
        expected_turn_id: None,
        proof_observed_at: None,
    };
    record_target_state(&mut target)?;
    // A target left in an older restart manifest may have been completed
    // manually since that failed run. If no new post-checkpoint work belongs to
    // this operation, completion is terminal and must not be revived or waited
    // on for three minutes.
    if target.state == ThreadRolloutState::CleanCompleted && !target.completed && !queued_once {
        crate::runtime_print!("RECOVERY_SKIPPED thread={id} reason=completed");
        return Ok(None);
    }
    Ok(Some(target))
}

pub(super) fn record_target_state_at(
    target: &mut RecoveryTarget,
    now: Instant,
) -> Result<(), String> {
    let was_started = target
        .observer
        .evidence
        .matches_expected_turn(target.expected_turn_id.as_deref());
    target.observer.poll()?;
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
    if started && !was_started && !target.execution_deadline_set {
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

pub(super) fn record_target_state(target: &mut RecoveryTarget) -> Result<(), String> {
    record_target_state_at(target, Instant::now())
}

pub(super) fn proof_survived_stability_window(observed_at: Instant, now: Instant) -> bool {
    now.duration_since(observed_at) >= RECOVERY_SOAK_WINDOW
}
