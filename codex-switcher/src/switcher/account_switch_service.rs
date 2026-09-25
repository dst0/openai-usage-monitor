use super::codex_availability_service::CodexAvailabilityService;
use super::*;
use crate::distribution::LogRedactionService;
use crate::models::AuthJson;
use crate::storage::{load_accounts, read_active_auth_json, save_accounts, write_active_auth_json};
use chrono::Utc;

pub(super) fn prioritize_primary(targets: &mut Vec<String>, primary: Option<&String>) {
    let Some(primary) = primary else { return };
    if let Some(index) = targets.iter().position(|id| id == primary) {
        targets.remove(index);
    }
    targets.insert(0, primary.clone());
}

pub(super) fn prioritize_primary_if_user(
    codex_home: &std::path::Path,
    targets: &mut Vec<String>,
    primary: Option<&String>,
) -> bool {
    if primary.is_some_and(|id| !is_user_thread(codex_home, id)) {
        return false;
    }
    prioritize_primary(targets, primary);
    true
}

pub fn switch_to_account(
    account_id: &str,
    restart_app: bool,
    notify: bool,
    trigger: SwitchTrigger,
) -> Result<SwitchOutcome, String> {
    let _operation = crate::recovery::operation_lock()?;
    let mut accounts_file = load_accounts()?;
    let target_idx = resolve_target_account_idx(&accounts_file.accounts, account_id)?;

    let target_account = accounts_file.accounts[target_idx].clone();

    // Guard: reject switching to an account that requires re-login until relogin is completed
    if target_account.needs_relogin() {
        let relogin_hint = target_account.name.as_deref().unwrap_or(&target_account.id);
        return Err(format!(
            "Account '{}' ({}) requires re-login before switching. Please run 'cxi relogin \"{}\"' first.",
            target_account.display_name(),
            target_account.email,
            relogin_hint
        ));
    }

    // Redundant switch guard: if target account is already active, return Ok(()) immediately.
    let active_id = accounts_file.active_account_id.as_deref();
    let is_already_active = active_id
        .map(|id| id.eq_ignore_ascii_case(&target_account.id))
        .unwrap_or(false)
        || accounts_file
            .accounts
            .iter()
            .find(|a| Some(a.id.as_str()) == active_id)
            .map(|a| a.id.eq_ignore_ascii_case(&target_account.id))
            .unwrap_or(false)
        || (target_account.tokens.refresh_token.is_some()
            && accounts_file
                .accounts
                .iter()
                .find(|a| Some(a.id.as_str()) == active_id)
                .and_then(|a| a.tokens.refresh_token.as_ref())
                == target_account.tokens.refresh_token.as_ref());

    if is_already_active {
        return Ok(SwitchOutcome {
            recovery_error: None,
        });
    }

    let app_was_running = restart_app && is_codex_app_running();
    if app_was_running
        && std::env::var_os("CODEX_RESTART_WORKER").is_some()
        && crate::recovery::restart_cancellation_requested()
    {
        return Err("Restart cancelled before Codex shutdown".into());
    }

    // Detect in-progress threads before gracefully terminating the app
    let running_threads = if app_was_running {
        let mut threads = detect_in_progress_threads();
        if let Ok(primary) =
            std::env::var("CODEX_PRIMARY_THREAD").or_else(|_| std::env::var("CODEX_THREAD_ID"))
        {
            let primary = clean_thread_id(&primary);
            let _ = prioritize_primary_if_user(
                &crate::storage::codex_home(),
                &mut threads,
                Some(&primary),
            );
        }
        if !threads.is_empty() {
            crate::runtime_print!(
                "📋 Detected {} active in-progress thread(s) before restart: {:?}",
                threads.len(),
                threads
            );
        }
        threads
    } else {
        Vec::new()
    };

    let previous_account_id = accounts_file
        .active_account_id
        .as_deref()
        .unwrap_or("unknown");
    crate::logger::log(
        "INFO",
        trigger.as_category(),
        &format!(
            "Starting account switch previous_ref={} target_ref={} target_email_ref={} [running_threads={}]",
            LogRedactionService::sanitize_field("account_id", previous_account_id),
            LogRedactionService::sanitize_field("account_id", &target_account.id),
            LogRedactionService::sanitize_field("email", &target_account.email),
            running_threads.len()
        ),
    );

    // 1. Prepare new auth.json
    let mut current_auth = read_active_auth_json().unwrap_or(AuthJson {
        auth_mode: Some("chatgpt".to_string()),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
    });
    let previous_auth = current_auth.clone();

    current_auth.tokens = Some(target_account.tokens.clone());
    current_auth.last_refresh = Some(Utc::now().to_rfc3339());

    let recovery_operation_id = if app_was_running {
        Some(crate::recovery::operation_id_for_banner("account_switch"))
    } else {
        None
    };
    let mut recovery_banner = if let Some(operation_id) = recovery_operation_id.as_deref() {
        crate::recovery::arm_automation_cooldown()?;
        Some(crate::recovery::RecoveryBanner::start(
            operation_id,
            &running_threads,
            "account_switch",
        )?)
    } else {
        None
    };
    if app_was_running && !running_threads.is_empty() {
        crate::recovery::preflight_desktop_dispatch()?;
    }

    // 2. Stop the desktop app before replacing credentials. A graceful exit is
    // the persistence boundary for active thread history and SQLite WAL state.
    // Never force-kill it: if it cannot flush and exit, leave auth untouched.
    if app_was_running {
        crate::recovery::save_pending(&running_threads)?;
        stop_codex_app_gracefully()?;
        // The first journal makes the target list durable before shutdown. The
        // second checkpoint is the verification boundary: it excludes work and
        // abort records flushed by the old Desktop from post-restart proof.
        if let Err(error) = crate::recovery::save_pending(&running_threads) {
            return Err(CodexAvailabilityService::relaunch_previous_state(error));
        }
    }

    // 3. Atomically write to ~/.codex/auth.json
    if let Err(error) = write_active_auth_json(&current_auth) {
        return Err(if app_was_running {
            CodexAvailabilityService::relaunch_previous_state(error)
        } else {
            error
        });
    }

    // 4. Update active_account_id in accounts.json. Restore the previous auth
    // if this second half of the local transaction fails.
    accounts_file.active_account_id = Some(target_account.id.clone());
    if let Err(error) = save_accounts(&accounts_file) {
        let restore_result = write_active_auth_json(&previous_auth);
        if app_was_running {
            let _ = launch_codex_app();
        }
        return match restore_result {
            Ok(()) => Err(error),
            Err(restore_error) => Err(format!(
                "Failed to update account state ({error}); restoring the previous authentication also failed ({restore_error})"
            )),
        };
    }

    // 5. Relaunch the desktop first, then dispatch through its own queue/UI.
    // A separate `codex exec resume` process would own the thread writer lock
    // and make the desktop show "This is open in another app".
    let recovery_error = if app_was_running {
        match launch_codex_app() {
            Ok(launched_pids) => {
                let restore_result = if launched_pids.len() != 1 {
                    Err(format!(
                        "Codex relaunch must produce exactly one main process, got {launched_pids:?}"
                    ))
                } else {
                    recovery_banner
                        .as_ref()
                        .expect("running app must have a recovery banner")
                        .restore_after_relaunch(
                            launched_pids[0],
                            recovery_operation_id.as_deref().unwrap_or("account_switch"),
                            "account_switch",
                        )
                };
                let recovery_result = restore_result.and_then(|()| {
                    crate::recovery::recover_threads_with_banner(
                        &running_threads,
                        crate::recovery::RecoveryMode::CapturedRestart,
                        recovery_banner.as_mut().unwrap(),
                    )
                });
                drop(recovery_banner.take());
                let stability_result = crate::recovery::verify_desktop_stable(&launched_pids, true);
                match (recovery_result, stability_result) {
                    (Ok(()), Ok(())) => None,
                    (Err(recovery), Ok(())) => Some(recovery),
                    (Ok(()), Err(stability)) => Some(stability),
                    (Err(recovery), Err(stability)) => Some(format!(
                        "{recovery}; desktop stability also failed: {stability}"
                    )),
                }
                .map(CodexAvailabilityService::keep_after_failure)
            }
            Err(error) => {
                drop(recovery_banner.take());
                Some(CodexAvailabilityService::keep_after_failure(error))
            }
        }
    } else {
        None
    };
    if app_was_running {
        crate::recovery::arm_automation_cooldown()?;
    }
    // 6. Send macOS user notification
    if notify {
        send_macos_notification(
            &format!("Switched to {}", target_account.display_name()),
            &format!("5h Limit: {:.0}%", target_account.last_primary_percentage),
        );
    }

    if let Some(ref err) = recovery_error {
        crate::logger::log(
            "WARN",
            trigger.as_category(),
            &format!(
                "Switched account_ref={} email_ref={} but recovery had error: {}",
                LogRedactionService::sanitize_field("account_id", &target_account.id),
                LogRedactionService::sanitize_field("email", &target_account.email),
                err
            ),
        );
    }

    crate::logger::log(
        "INFO",
        trigger.as_category(),
        &format!(
            "Successfully switched account_ref={} email_ref={} [recovery_error={:?}]",
            LogRedactionService::sanitize_field("account_id", &target_account.id),
            LogRedactionService::sanitize_field("email", &target_account.email),
            recovery_error
        ),
    );

    Ok(SwitchOutcome { recovery_error })
}
