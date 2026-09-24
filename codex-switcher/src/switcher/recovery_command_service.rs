use super::account_switch_service::prioritize_primary_if_user;
use super::codex_app_lifecycle::{codex_app_pids, CODEX_APP_EXECUTABLE};
use super::codex_availability_service::CodexAvailabilityService;
use super::*;
use chrono::Utc;
use std::process::Command;
use std::thread::sleep;
use std::time::Duration;

/// Recovery-only entry point. Uses the same verified pipeline as account switching.
pub fn resume_thread_interactive(thread_id: Option<&str>) -> Result<(), String> {
    let _operation = crate::recovery::operation_lock()?;
    if !is_codex_app_running() {
        return Err("Codex is not running; launch it before using resume".into());
    }
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
    crate::recovery::recover_threads(&targets, mode)
}

/// Self-restart is supported: a detached worker, rather than the app's child
/// shell, owns the operation so recovery survives termination of this host.
pub fn dispatch_self_restart(args: &[String]) -> Result<bool, String> {
    if std::env::var_os("CODEX_RESTART_WORKER").is_some() {
        return Ok(false);
    }
    let output = Command::new("/bin/ps")
        .args(["-axo", "pid=,ppid=,comm="])
        .output()
        .map_err(|e| e.to_string())?;
    if !output.status.success() {
        return Err("Cannot verify restart worker ancestry".into());
    }
    if !has_codex_ancestor(&String::from_utf8_lossy(&output.stdout), std::process::id())? {
        return Ok(false);
    }
    let home = crate::storage::codex_home();
    let label = "com.codex.switcher.restart-worker";
    let previous = Command::new("launchctl")
        .args(["list", label])
        .output()
        .map_err(|e| e.to_string())?;
    if previous.status.success() {
        if String::from_utf8_lossy(&previous.stdout).contains("\"PID\"") {
            return Err("A restart worker is already running".into());
        }
        let removed = Command::new("launchctl")
            .args(["remove", label])
            .status()
            .map_err(|e| e.to_string())?;
        if !removed.success() {
            return Err("Could not retire the completed restart worker".into());
        }
    }
    crate::recovery::arm_automation_cooldown()?;
    crate::recovery::clear_restart_cancellation()?;
    let directory = home.join("recovery-runs");
    std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
    let operation_id = format!("{}-{}", Utc::now().timestamp_millis(), std::process::id());
    let log = directory.join(format!("restart-{operation_id}.log"));
    use std::os::unix::fs::OpenOptionsExt;
    std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(&log)
        .map_err(|e| e.to_string())?;
    let executable = std::env::current_exe().map_err(|e| e.to_string())?;
    let mut submit = Command::new("launchctl");
    submit
        .args(["submit", "-l", label, "-o"])
        .arg(&log)
        .arg("-e")
        .arg(&log)
        // A submitted job can be relaunched even after a short successful run.
        // Remove the one-shot label from inside the wrapper after the child
        // finishes. The operation claim remains the destructive at-most-once
        // guard if the wrapper itself is interrupted before that cleanup.
        .args([
            "--",
            "/bin/sh",
            "-c",
            "\"$@\"; /bin/launchctl remove com.codex.switcher.restart-worker >/dev/null 2>&1; exit 0",
            "codex-restart-once",
            "/usr/bin/env",
            "CODEX_RESTART_WORKER=1",
        ])
        .arg(format!("CODEX_RESTART_OPERATION={operation_id}"))
        .arg(format!("CODEX_HOME={}", home.display()));
    if let Ok(primary) = std::env::var("CODEX_THREAD_ID") {
        submit.arg(format!(
            "CODEX_PRIMARY_THREAD={}",
            clean_thread_id(&primary)
        ));
    }
    let status = submit
        .arg(executable)
        .arg("--restart-worker")
        .args(args)
        .status()
        .map_err(|e| e.to_string())?;
    if !status.success() {
        return Err("Could not launch independent restart worker".into());
    }
    crate::runtime_print!(
        "RESTART_DISPATCHED job={label} log={} (scheduled, not yet verified)",
        log.display()
    );
    Ok(true)
}

pub(super) fn has_codex_ancestor(processes: &str, mut pid: u32) -> Result<bool, String> {
    let rows: Vec<_> = processes
        .lines()
        .filter_map(|line| {
            let mut fields = line.trim().splitn(3, char::is_whitespace);
            let id = fields.next()?.parse::<u32>().ok()?;
            let rest = line.trim().strip_prefix(&id.to_string())?.trim_start();
            let split = rest.find(char::is_whitespace)?;
            Some((id, rest[..split].parse::<u32>().ok()?, rest[split..].trim()))
        })
        .collect();
    for _ in 0..128 {
        if pid <= 1 {
            return Ok(false);
        }
        let (_, parent, executable) = rows
            .iter()
            .find(|row| row.0 == pid)
            .ok_or("Cannot resolve restart worker ancestry")?;
        if *executable == CODEX_APP_EXECUTABLE {
            return Ok(true);
        }
        pid = *parent;
    }
    Err("Cycle in restart worker ancestry".into())
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
    sleep(Duration::from_secs(delay_seconds));
    if std::env::var_os("CODEX_RESTART_WORKER").is_some()
        && crate::recovery::restart_cancellation_requested()
    {
        crate::runtime_print!("WORKER_CANCELLED phase=pre_shutdown");
        return Ok(());
    }
    let _operation = crate::recovery::operation_lock()?;
    crate::recovery::arm_automation_cooldown()?;
    if !is_codex_app_running() {
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
        codex_app_pids(),
        targets
    );
    let operation_id = crate::recovery::operation_id_for_banner("captured_restart");
    let banner =
        crate::recovery::RecoveryBanner::start(&operation_id, &targets, "captured_restart")?;
    if !targets.is_empty() {
        crate::recovery::preflight_desktop_dispatch()?;
    }
    crate::recovery::save_pending(&targets)?;
    stop_codex_app_gracefully()?;
    // Re-checkpoint only after the old process has fully exited, so recovery
    // cannot be falsely verified by work flushed during shutdown.
    crate::recovery::save_pending(&targets)?;
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
        banner
            .restore_after_relaunch(launched_pids[0], &operation_id, "captured_restart")
            .and_then(|()| {
                crate::recovery::recover_threads_with_banner(
                    &targets,
                    crate::recovery::RecoveryMode::CapturedRestart,
                    &banner,
                )
            })
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
