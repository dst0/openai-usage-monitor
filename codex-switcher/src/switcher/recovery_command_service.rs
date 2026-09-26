use super::active_auth_registry_sync_service::ActiveAuthRegistrySyncService;
use super::codex_availability_service::CodexAvailabilityService;
use super::desktop_session_binding_service::DesktopSessionBindingService;
use super::primary_target_selection::prioritize_primary_if_user;
use super::restart_worker_dispatch_service::RestartWorkerDispatchService;
use super::*;
use std::time::Duration;

/// Recovery-only entry point. Uses the same verified pipeline as account switching.
pub fn resume_thread_interactive(thread_id: Option<&str>) -> Result<(), String> {
    let _operation = crate::recovery::operation_lock()?;
    let (targets, mode) = match thread_id {
        Some(tid) => (
            vec![clean_thread_id(tid)],
            crate::recovery::RecoveryMode::ExplicitTarget,
        ),
        None => (
            detect_in_progress_threads(),
            crate::recovery::RecoveryMode::DiscoveredOnly,
        ),
    };
    let desktop_running = is_codex_app_running_checked()?;
    if !desktop_running {
        let home = crate::storage::codex_home();
        if targets.is_empty() {
            crate::runtime_print!("RECOVERY_RESULT verified_or_completed=0 failed=0");
            return Ok(());
        }
        if !CodexAvailabilityService::resume_has_launchable_target(&targets, |id| {
            is_user_thread(&home, id)
        }) {
            return Err(
                "Target is absent, archived, or a subagent; Codex was not started for recovery"
                    .into(),
            );
        }
    }
    CodexAvailabilityService::ensure_running_for_resume(|| desktop_running, launch_codex_app)?;
    crate::recovery::recover_threads(&targets, mode)
}

/// Dispatch a restart outside the Desktop process when the caller is its child.
pub fn dispatch_self_restart(args: &[String]) -> Result<bool, String> {
    RestartWorkerDispatchService::dispatch(args)
}

#[cfg(test)]
pub(super) fn has_codex_ancestor(processes: &str, pid: u32) -> Result<bool, String> {
    RestartWorkerDispatchService::has_codex_ancestor(processes, pid)
}

pub fn restart_and_recover(
    delay_seconds: u64,
    primary_thread: Option<String>,
) -> Result<(), String> {
    let primary = primary_thread
        .or_else(|| std::env::var("CODEX_THREAD_ID").ok())
        .map(|id| clean_thread_id(&id));
    let mut args = vec![
        "restart".into(),
        "--delay-seconds".into(),
        delay_seconds.max(5).to_string(),
    ];
    if let Some(id) = &primary {
        args.extend(["--primary-thread".into(), id.clone()]);
    }
    if std::env::var_os("CODEX_RESTART_WORKER").is_none() && dispatch_self_restart(&args)? {
        return Ok(());
    }
    std::thread::sleep(Duration::from_secs(delay_seconds));
    if std::env::var_os("CODEX_RESTART_WORKER").is_some()
        && crate::recovery::restart_cancellation_requested()
    {
        crate::runtime_print!("WORKER_CANCELLED phase=pre_shutdown");
        return Ok(());
    }
    let _operation = crate::recovery::operation_lock()?;
    crate::recovery::arm_automation_cooldown()?;
    let cli_account_id = DesktopSessionBindingService::verified_cli_account_id()?;
    if !is_codex_app_running_checked()? {
        return Err("Codex is not running".into());
    }
    let mut targets = detect_in_progress_threads();
    if !prioritize_primary_if_user(
        &crate::storage::codex_home(),
        &mut targets,
        primary.as_ref(),
    ) {
        return Err("Primary task is absent, archived, or a subagent; refusing restart".into());
    }
    crate::runtime_print!(
        "RESTART_BEGIN old_pids={:?} targets={:?}",
        current_codex_app_pids_checked()?,
        targets
    );
    let operation_id = crate::recovery::operation_id_for_banner("captured_restart");
    let mut banner =
        crate::recovery::RecoveryBanner::start(&operation_id, &targets, "captured_restart")?;
    if !targets.is_empty() {
        crate::recovery::preflight_desktop_dispatch()?;
    }
    let expected = banner.expected_process().clone();
    preflight_shutdown_windows(&expected)?;
    let checkpoint = crate::recovery::RecoveryManifestSnapshot::capture()?;
    crate::recovery::save_pending(&targets).map_err(|error| checkpoint.rollback_error(error))?;
    preflight_shutdown_windows(&expected).map_err(|error| checkpoint.rollback_error(error))?;
    if let Err(error) = stop_codex_app_gracefully(&expected) {
        return Err(if error.before_signal {
            checkpoint.rollback_error(error.to_string())
        } else {
            error.to_string()
        });
    }
    // Re-checkpoint only after the old process has fully exited, so recovery
    // cannot be falsely verified by work flushed during shutdown.
    if let Err(error) = crate::recovery::save_pending(&targets) {
        drop(banner);
        return Err(CodexAvailabilityService::relaunch_previous_state(error));
    }
    let current_account = (|| -> Result<String, String> {
        let mut accounts = crate::storage::load_accounts()?;
        ActiveAuthRegistrySyncService::sync_from_disk(&mut accounts)?
            .ok_or("Desktop authentication disappeared after shutdown")?;
        accounts
            .active_account_id
            .ok_or("Active Desktop account identity is unavailable".into())
    })();
    let current_account = match current_account {
        Ok(id) => id,
        Err(error) => {
            drop(banner);
            return Err(CodexAvailabilityService::relaunch_previous_state(error));
        }
    };
    if current_account != cli_account_id {
        drop(banner);
        return Err(CodexAvailabilityService::relaunch_previous_state(
            "CLI account changed during Desktop restart".into(),
        ));
    }
    let launched_pids = match launch_codex_app() {
        Ok(pids) => pids,
        Err(error) => {
            drop(banner);
            return Err(CodexAvailabilityService::keep_after_failure(error));
        }
    };
    crate::runtime_print!("RESTART_LAUNCHED new_pids={launched_pids:?}");
    let recovery_result = if launched_pids.len() != 1 {
        Err(format!(
            "Codex relaunch must produce exactly one main process, got {launched_pids:?}"
        ))
    } else {
        DesktopSessionBindingService::bind_then_recover(
            &crate::storage::codex_home(),
            &cli_account_id,
            launched_pids[0],
            |bound_process| {
                if DesktopSessionBindingService::verified_cli_account_id()? != cli_account_id {
                    return Err("CLI account changed during Desktop restart".into());
                }
                banner.restore_after_relaunch(
                    launched_pids[0],
                    &operation_id,
                    "captured_restart",
                )?;
                crate::recovery::recover_threads_with_banner(
                    &targets,
                    crate::recovery::RecoveryMode::CapturedRestart,
                    &mut banner,
                )?;
                DesktopSessionBindingService::confirm_after_recovery(bound_process)
            },
        )
    };
    drop(banner);
    let stability_result = crate::recovery::verify_desktop_stable(&launched_pids, true);
    match (recovery_result, stability_result) {
        (Ok(()), Ok(())) => {}
        (Err(recovery), Ok(())) => {
            return Err(CodexAvailabilityService::keep_after_failure(recovery))
        }
        (Ok(()), Err(stability)) => {
            return Err(CodexAvailabilityService::keep_after_failure(stability))
        }
        (Err(recovery), Err(stability)) => {
            return Err(CodexAvailabilityService::keep_after_failure(format!(
                "{recovery}; desktop stability also failed: {stability}"
            )))
        }
    };
    for target in &targets {
        match inspect_thread_rollout_state(&crate::storage::codex_home(), target) {
            ThreadRolloutState::ActiveInProgress | ThreadRolloutState::CleanCompleted => {}
            state => {
                return Err(format!(
                    "Recovered task {target} ended in delayed state {state:?} during stabilization"
                ))
            }
        }
    }
    crate::recovery::arm_automation_cooldown()
}
