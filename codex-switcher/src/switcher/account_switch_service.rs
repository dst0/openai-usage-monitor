use super::account_switch_auth_service::AccountSwitchAuthService;
use super::account_switch_commit_service::AccountSwitchCommitService;
use super::account_target_resolver::resolve_account_with_sync;
use super::codex_availability_service::CodexAvailabilityService;
use super::desktop_session_binding_service::DesktopSessionBindingService;
use super::direct_switch_journal::{reconcile_pending_direct_switch, DirectSwitchJournal};
use super::*;
use crate::distribution::LogRedactionService;
use crate::storage::load_accounts;

pub fn switch_to_account(
    account_id: &str,
    restart_app: bool,
    notify: bool,
    trigger: SwitchTrigger,
) -> Result<SwitchOutcome, String> {
    let _operation = crate::recovery::operation_lock()?;
    reconcile_pending_direct_switch()?;
    let mut accounts_file = load_accounts()?;
    // Desktop owns refresh-token rotation; sync before selecting credentials.
    let (target_account, auth_before_stop) = resolve_account_with_sync(
        &mut accounts_file,
        account_id,
        ActiveAuthRegistrySyncService::sync_from_disk,
    )?;
    let desktop_running = is_codex_app_running_checked()?;
    if is_shared_auth_active_checked()? && !desktop_running {
        return Err(
            "A bundled Desktop credential writer is running without its main process".into(),
        );
    }
    if target_account.needs_relogin() {
        let relogin_hint = target_account.name.as_deref().unwrap_or(&target_account.id);
        return Err(format!(
            "Account '{}' ({}) requires re-login before switching. Please run 'cxi relogin \"{}\"' first.",
            target_account.display_name(),
            target_account.email,
            relogin_hint
        ));
    }
    if desktop_running && auth_before_stop.is_none() {
        return Err("Running Desktop has no readable authentication".into());
    }
    let is_already_active = auth_before_stop.is_some()
        && accounts_file
            .active_account_id
            .as_deref()
            .is_some_and(|id| id.eq_ignore_ascii_case(&target_account.id));

    if is_already_active {
        return Ok(SwitchOutcome {
            recovery_error: None,
        });
    }

    if desktop_running && !restart_app {
        return Err(
            "Cannot change shared authentication while ChatGPT Desktop is running without a restart"
                .into(),
        );
    }

    let app_was_running = restart_app && desktop_running;
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
        let expected = recovery_banner
            .as_ref()
            .expect("running app must have a banner")
            .expected_process()
            .clone();
        preflight_shutdown_windows(&expected)?;
        let checkpoint = crate::recovery::RecoveryManifestSnapshot::capture()?;
        crate::recovery::save_pending(&running_threads)
            .map_err(|error| checkpoint.rollback_error(error))?;
        preflight_shutdown_windows(&expected).map_err(|error| checkpoint.rollback_error(error))?;
        if let Err(error) = stop_codex_app_gracefully(&expected) {
            return Err(if error.before_signal {
                checkpoint.rollback_error(error.to_string())
            } else {
                error.to_string()
            });
        }
        // The first journal makes the target list durable before shutdown. The
        // second checkpoint is the verification boundary: it excludes work and
        // abort records flushed by the old Desktop from post-restart proof.
        if let Err(error) = crate::recovery::save_pending(&running_threads) {
            return Err(CodexAvailabilityService::relaunch_previous_state(error));
        }
    }

    let (previous_auth, committed_auth) = AccountSwitchAuthService::replace(
        &target_account,
        app_was_running,
        auth_before_stop,
        &mut accounts_file,
    )?;

    // 4. Update active_account_id in accounts.json. Restore the previous auth
    // if this second half of the local transaction fails.
    if let Err(error) = AccountSwitchCommitService::commit(
        &target_account,
        accounts_file.active_account_id.as_deref(),
    ) {
        return Err(AccountSwitchAuthService::rollback_after_commit_failure(
            previous_auth.as_ref(),
            &committed_auth,
            app_was_running,
            error,
        ));
    }
    AccountSwitchCommitService::verify_auth_after_commit(&committed_auth)?;
    DirectSwitchJournal::verify_target_and_clear(&crate::storage::codex_home())?;

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
                    DesktopSessionBindingService::bind_launched(
                        &crate::storage::codex_home(),
                        &target_account.id,
                        launched_pids[0],
                    )
                    .and_then(|_| {
                        recovery_banner
                            .as_ref()
                            .expect("running app must have a recovery banner")
                            .restore_after_relaunch(
                                launched_pids[0],
                                recovery_operation_id.as_deref().unwrap_or("account_switch"),
                                "account_switch",
                            )
                    })
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
