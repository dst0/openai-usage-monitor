use super::daemon_tick_service::DaemonTickService;
use super::log_permissions_service::LogPermissionsService;
use super::log_redaction_service::LogRedactionService;
use crate::models::AccountsFile;
use crate::storage::{daemon_lock_path, load_accounts};
use fs2::FileExt;
use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct DaemonLoopService;

impl DaemonLoopService {
    pub fn watchdog_needs_immediate_check() -> bool {
        let Ok(accounts_file) = load_accounts() else {
            return false;
        };
        Self::watchdog_needs_immediate_check_with(&accounts_file, || {
            !crate::switcher::detect_quota_blocked_user_threads_since(30).is_empty()
        })
    }

    fn watchdog_needs_immediate_check_with(
        accounts_file: &AccountsFile,
        recent_quota_blocked: impl FnOnce() -> bool,
    ) -> bool {
        let settings = &accounts_file.settings;
        if !settings.auto_switch_enabled && !settings.auto_reset_weekly_enabled {
            return false;
        }
        let active = accounts_file
            .active_account_id
            .as_ref()
            .and_then(|active_id| {
                accounts_file
                    .accounts
                    .iter()
                    .find(|account| account.id == *active_id)
            });
        if active.is_some_and(|account| {
            crate::strategy::is_account_depleted(account, settings.switch_threshold_percent)
        }) {
            return true;
        }
        // Weekly reset is independent of account switching and retains the
        // existing rapid blocked-task probe when enabled on its own.
        recent_quota_blocked()
    }

    pub fn run() {
        std::env::set_var("CODEX_MONITOR_BACKGROUND", "1");
        if let Err(error) = LogPermissionsService::enforce() {
            LogRedactionService::eprint_background(&format!(
                "Daemon startup refused: monitor log permissions are unsafe ({error})"
            ));
            return;
        }
        LogRedactionService::print_background(
            "🚀 Starting Codex Usage Monitor & Switcher Daemon...",
        );
        let lock_path = daemon_lock_path();
        if let Some(parent) = lock_path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }
        let lock_file = match OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
        {
            Ok(file) => file,
            Err(error) => {
                LogRedactionService::eprint_background(&format!(
                    "Failed to open daemon lockfile: {error}"
                ));
                return;
            }
        };
        if lock_file.try_lock_exclusive().is_err() {
            LogRedactionService::eprint_background(&format!(
                "⚠️ Codex switcher daemon is already running (lock held at {}).",
                lock_path.display()
            ));
            return;
        }

        let mut deferred_recovery = crate::recovery::DeferredRecoveryService::new();

        loop {
            let _ = crate::logger::rotate_all_logs(
                crate::logger::DEFAULT_MAX_LOG_SIZE,
                crate::logger::DEFAULT_MAX_ARCHIVES,
            );
            if let Err(error) = DaemonTickService::run(true) {
                LogRedactionService::eprint_background(&format!("Error in daemon tick: {error}"));
                crate::logger::log("ERROR", "DAEMON", "Daemon tick failed");
            }
            deferred_recovery.poll();

            let last_auth_mtime = auth_mtime();
            let interval_secs = load_accounts()
                .map(|accounts| accounts.settings.poll_interval_seconds.max(5))
                .unwrap_or(60);
            let sleep_start = Instant::now();
            let target_duration = Duration::from_secs(interval_secs);
            let mut watchdog_ticks = 0_u32;
            while sleep_start.elapsed() < target_duration {
                sleep(Duration::from_secs(1));
                deferred_recovery.poll();
                watchdog_ticks = watchdog_ticks.wrapping_add(1);
                let current_auth_mtime = auth_mtime();
                if current_auth_mtime != last_auth_mtime && current_auth_mtime.is_some() {
                    break;
                }
                if watchdog_ticks.is_multiple_of(2) && Self::watchdog_needs_immediate_check() {
                    crate::logger::log(
                        "INFO",
                        "WATCHDOG",
                        "Immediate quota exhaustion or blocked thread detected",
                    );
                    break;
                }
            }
        }
    }
}

fn auth_mtime() -> Option<std::time::SystemTime> {
    std::fs::metadata(crate::storage::auth_json_path())
        .and_then(|metadata| metadata.modified())
        .ok()
}

#[cfg(test)]
#[path = "daemon_loop_service.test.rs"]
mod tests;
