use super::{
    automation_guard::operation_id_for_banner,
    desktop_ipc::DesktopIpc,
    foreground_checkpoint_service::ForegroundCheckpointService,
    manifest_store::{
        finalize_target, load_manifest, prune_ineligible_targets, recovery_account_binding,
        write_manifest,
    },
    recovery_banner::RecoveryBanner,
    recovery_checkpoint::checkpoint_targets,
    recovery_mode::RecoveryMode,
    recovery_target::{
        prepare_target, record_target_state_with_budget, RecoveryTarget,
        FOREGROUND_SCAN_BUDGET_BYTES, RECOVERY_DISPATCH_TIMEOUT, RECOVERY_EXECUTION_TIMEOUT,
    },
    target_dispatch::dispatch_if_needed,
};
use crate::{recovery_banner::BannerSessionStatus, storage, switcher};
use std::{
    thread::sleep,
    time::{Duration, Instant},
};
const IPC_STARTUP_TIMEOUT: Duration = Duration::from_secs(90);

pub fn recover_threads(ids: &[String], mode: RecoveryMode) -> Result<(), String> {
    let operation_id = operation_id_for_banner("thread_recovery");
    let mut banner =
        RecoveryBanner::start_for_running_desktop(&operation_id, ids, "thread_recovery")?;
    recover_threads_with_banner(ids, mode, &mut banner)
}

pub(crate) fn recover_threads_with_banner(
    ids: &[String],
    mode: RecoveryMode,
    banner: &mut RecoveryBanner,
) -> Result<(), String> {
    let home = storage::codex_home();
    let mut pending_manifest = load_manifest()?;
    let binding = recovery_account_binding(matches!(
        mode,
        RecoveryMode::DeferredOwned | RecoveryMode::DeferredCaptured
    ));
    let (previous_pending, claimed) =
        checkpoint_targets(&home, &mut pending_manifest, ids, mode, binding.as_deref())?;
    write_manifest(&pending_manifest)?;

    // Establish all checkpoints before actions in any target, so early work in
    // target N cannot be missed while target 1 is being dispatched.
    let mut targets = Vec::new();
    let mut completed_without_action = Vec::new();
    let mut preparation_failures = Vec::new();
    let per_target_budget = FOREGROUND_SCAN_BUDGET_BYTES / ids.len().max(1) as u64;
    for id in ids {
        banner.record_status(id, BannerSessionStatus::InProgress);
        let previous = previous_pending.iter().find(|target| target.id == *id);
        match prepare_target(
            &home,
            id,
            previous.and_then(|target| target.offset),
            per_target_budget,
        ) {
            Ok(Some(target)) => targets.push(target),
            Ok(None) => {
                banner.record_status(id, BannerSessionStatus::Skipped);
                completed_without_action.push(id.clone());
            }
            Err(error) => {
                crate::runtime_error!("RECOVERY_FAILED thread={id} reason={error}");
                banner.record_status(id, BannerSessionStatus::Failed);
                preparation_failures.push(id.clone());
            }
        }
    }

    // Inspect the full checkpoint interval in fair bounded steps before any
    // owner-routed request; otherwise an unseen user turn could be duplicated.
    for id in ForegroundCheckpointService::new(&mut targets).scan_until_ready() {
        banner.record_status(&id, BannerSessionStatus::Skipped);
        completed_without_action.push(id);
    }

    // Route each turn through its Desktop owner. Cold tasks are mounted only
    // after owner discovery says they are not already owned; no fixed UI sleep
    // is treated as a readiness contract.
    if targets
        .iter()
        .any(|target| !target.completed && target.failure.is_none())
    {
        let per_target_dispatch_budget = FOREGROUND_SCAN_BUDGET_BYTES / targets.len().max(1) as u64;
        match DesktopIpc::connect_with_retry(IPC_STARTUP_TIMEOUT) {
            Ok(mut desktop) => {
                crate::runtime_print!("RECOVERY_CHANNEL_READY transport=desktop_ipc");
                for target in &mut targets {
                    let mut scan_budget = per_target_dispatch_budget;
                    let target_id = target.id.clone();
                    let before_send = || {
                        banner.ensure_visible_after_owner(ids, mode)?;
                        banner.record_status(&target_id, BannerSessionStatus::InProgress);
                        Ok(())
                    };
                    if let Err(error) = dispatch_if_needed(
                        &home,
                        &mut desktop,
                        target,
                        mode,
                        &mut scan_budget,
                        before_send,
                    ) {
                        mark_dispatch_failure(target, &error);
                    }
                }
                drop(desktop);
            }
            Err(error) => {
                for target in &mut targets {
                    if record_target_state_with_budget(target, per_target_dispatch_budget).is_err()
                        || !target.scan_complete
                        || (!target.completed && !target.observer.evidence.started)
                    {
                        // No IPC request was sent. Retain the original
                        // checkpoint for a later owner-verified attempt.
                        mark_pre_dispatch_channel_failure(target, &error);
                    }
                }
            }
        }
    }

    // A cold task is mounted only after authoritative owner discovery reports
    // no client. If that changed the visible task, restore the primary once;
    // never cycle through already-owned tasks or navigate again after proof.
    if let Some(primary) = ids.first() {
        let last_mounted = targets
            .iter()
            .rev()
            .find(|target| target.mounted_by_recovery)
            .map(|target| target.id.as_str());
        if last_mounted.is_some_and(|mounted| mounted != primary) {
            if let Err(error) = switcher::open_thread_in_codex(primary) {
                crate::runtime_error!("RECOVERY_PRIMARY_NAVIGATION_FAILED reason={error}");
            }
        }
    }

    loop {
        let per_target_observation_budget =
            FOREGROUND_SCAN_BUDGET_BYTES / targets.len().max(1) as u64;
        for target in &mut targets {
            if target.failure.is_some() || target.completed {
                continue;
            }
            if let Err(error) =
                record_target_state_with_budget(target, per_target_observation_budget)
            {
                target.failure = Some(error);
            } else if (target.dispatched || target.execution_deadline_set)
                && Instant::now() >= target.deadline
            {
                target.failure = Some(if target.observer.evidence.started {
                    format!(
                        "Task started but produced no new agent work within {}s",
                        RECOVERY_EXECUTION_TIMEOUT.as_secs()
                    )
                } else if target.writer_locked {
                    format!(
                        "Desktop held the writer lock but produced no new agent work within {}s",
                        RECOVERY_EXECUTION_TIMEOUT.as_secs()
                    )
                } else {
                    format!(
                        "Desktop accepted recovery but no task started within {}s; dispatch was not retried",
                        RECOVERY_DISPATCH_TIMEOUT.as_secs()
                    )
                });
            }
        }
        if targets
            .iter()
            .all(|target| target.failure.is_some() || target.completed)
        {
            break;
        }
        sleep(Duration::from_millis(500));
    }

    let mut failures = preparation_failures.clone();
    for target in &mut targets {
        if !target.completed && target.failure.is_none() {
            target.failure = Some("Recovery ended without verified agent work".into());
        }
        if let Some(error) = &target.failure {
            crate::runtime_error!("RECOVERY_FAILED thread={} reason={error}", target.id);
            banner.record_status(&target.id, BannerSessionStatus::Failed);
            failures.push(target.id.clone());
        } else if target.completed {
            banner.record_status(&target.id, BannerSessionStatus::Completed);
        }
        finalize_target(
            &mut pending_manifest,
            &target.id,
            target.owner_unavailable,
            target.dispatched,
            binding.as_deref(),
            target.account_mismatch || claimed.contains(&target.id),
        );
    }
    for id in &preparation_failures {
        finalize_target(
            &mut pending_manifest,
            id,
            true,
            false,
            binding.as_deref(),
            claimed.contains(id),
        );
    }
    for id in completed_without_action {
        pending_manifest.retain(|item| item.id != id);
    }
    if let Err(error) = prune_ineligible_targets(&home, &mut pending_manifest) {
        // Keep pre-dispatch ownerless intent on disk if queue/rollout state is
        // temporarily unreadable. The deferred worker will revalidate later.
        write_manifest(&pending_manifest)?;
        return Err(error);
    }
    write_manifest(&pending_manifest)?;

    failures.sort();
    failures.dedup();
    crate::runtime_print!(
        "RECOVERY_RESULT verified_or_completed={} failed={}",
        ids.len().saturating_sub(failures.len()),
        failures.len()
    );
    crate::logger::log(
        if failures.is_empty() { "INFO" } else { "WARN" },
        "RECOVERY",
        &format!(
            "RECOVERY_RESULT verified_or_completed={} failed={}",
            ids.len().saturating_sub(failures.len()),
            failures.len()
        ),
    );
    if failures.is_empty() {
        Ok(())
    } else {
        Err(format!(
            "Recovery unverified for {} thread(s): {}",
            failures.len(),
            failures.join(", ")
        ))
    }
}

pub(super) fn mark_pre_dispatch_channel_failure(target: &mut RecoveryTarget, error: &str) {
    target.owner_unavailable = true;
    target.failure = Some(error.to_owned());
}

pub(super) fn mark_dispatch_failure(target: &mut RecoveryTarget, error: &str) {
    if !target.dispatched && !target.account_mismatch {
        // SQLite/queue checks and owner resolution are all before the IPC
        // send. Keep the checkpoint, but never retry an uncertain send.
        target.owner_unavailable = true;
    }
    target.failure = Some(error.to_owned());
}
