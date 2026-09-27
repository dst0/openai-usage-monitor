use super::active_auth_registry_sync_service::ActiveAuthRegistrySyncService;
use super::codex_availability_service::CodexAvailabilityService;
use super::desktop_session_binding_service::DesktopSessionBindingService;
use super::primary_target_selection::prioritize_primary_if_user;
use super::restart_window_task_service::RestartWindowTaskService;
use super::restart_worker_args_service::RestartWorkerArgsService;
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

/// `restore_window_tasks` is the user's explicit request to capture each
/// window's selected task and reopen it after the relaunch; without it more
/// than one window refuses the restart.
pub fn restart_and_recover(
    delay_seconds: u64,
    primary_thread: Option<String>,
    restore_window_tasks: bool,
) -> Result<(), String> {
    let primary = primary_thread
        .or_else(|| std::env::var("CODEX_THREAD_ID").ok())
        .map(|id| clean_thread_id(&id));
    let args =
        RestartWorkerArgsService::restart(delay_seconds, primary.as_deref(), restore_window_tasks);
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
    let mut window_tasks =
        RestartWindowTaskService::capture_if_requested(restore_window_tasks, &expected)?;
    let captured = RestartWindowTaskService::captured_windows(&window_tasks);
    preflight_shutdown_windows(&expected, captured.as_deref())?;
    let checkpoint = crate::recovery::RecoveryManifestSnapshot::capture()?;
    crate::recovery::save_pending(&targets).map_err(|error| checkpoint.rollback_error(error))?;
    preflight_shutdown_windows(&expected, captured.as_deref())
        .map_err(|error| checkpoint.rollback_error(error))?;
    if let Err(error) = stop_codex_app_gracefully(&expected, captured.as_deref()) {
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
        let error = CodexAvailabilityService::relaunch_previous_state(error);
        return Err(RestartWindowTaskService::with_windows(error, window_tasks));
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
            let error = CodexAvailabilityService::relaunch_previous_state(error);
            return Err(RestartWindowTaskService::with_windows(error, window_tasks));
        }
    };
    if current_account != cli_account_id {
        drop(banner);
        let error = CodexAvailabilityService::relaunch_previous_state(
            "CLI account changed during Desktop restart".into(),
        );
        return Err(RestartWindowTaskService::with_windows(error, window_tasks));
    }
    let launched_pids = match launch_codex_app() {
        Ok(pids) => pids,
        Err(error) => {
            drop(banner);
            let error = CodexAvailabilityService::keep_after_failure(error);
            return Err(RestartWindowTaskService::with_windows(error, window_tasks));
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
                RestartWindowTaskService::around_recovery(
                    &mut window_tasks,
                    bound_process,
                    &targets,
                    || {
                        crate::recovery::recover_threads_with_banner(
                            &targets,
                            crate::recovery::RecoveryMode::CapturedRestart,
                            &mut banner,
                        )
                    },
                )?;
                DesktopSessionBindingService::confirm_after_recovery(bound_process)
            },
        )
    };
    drop(banner);
    let stability_result = crate::recovery::verify_desktop_stable(&launched_pids, true);
    let windows = RestartWindowTaskService::finish(window_tasks);
    let failure = match (recovery_result, stability_result) {
        (Ok(()), Ok(())) => None,
        (Err(recovery), Ok(())) => Some(recovery),
        (Ok(()), Err(stability)) => Some(stability),
        (Err(recovery), Err(stability)) => Some(format!(
            "{recovery}; desktop stability also failed: {stability}"
        )),
    };
    if let Some(failure) = failure {
        let failure = RestartWindowTaskService::append_failure(Some(failure), windows);
        return Err(CodexAvailabilityService::keep_after_failure(
            failure.unwrap_or_default(),
        ));
    }
    for target in &targets {
        match inspect_thread_rollout_state(&crate::storage::codex_home(), target) {
            ThreadRolloutState::ActiveInProgress | ThreadRolloutState::CleanCompleted => {}
            state => {
                let delayed = format!(
                    "Recovered task {target} ended in delayed state {state:?} during stabilization"
                );
                return Err(
                    RestartWindowTaskService::append_failure(Some(delayed), windows)
                        .unwrap_or_default(),
                );
            }
        }
    }
    // Desktop and recovery are fine: re-arm the cooldown before reporting
    // windows that did not come back.
    crate::recovery::arm_automation_cooldown()?;
    windows
}
