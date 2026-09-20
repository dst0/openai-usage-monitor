use super::daemon_tick_service::DaemonTickService;
use super::log_permissions_service::LogPermissionsService;
use super::log_redaction_service::LogRedactionService;
use crate::storage::{daemon_lock_path, load_accounts};
use fs2::FileExt;
use std::fs::OpenOptions;
use std::thread::sleep;
use std::time::{Duration, Instant};

pub struct DaemonLoopService;

impl DaemonLoopService {
    pub fn watchdog_needs_immediate_check() -> bool {
        if let Ok(accounts_file) = load_accounts() {
            if let Some(active_id) = &accounts_file.active_account_id {
                if let Some(active) = accounts_file.accounts.iter().find(|a| a.id == *active_id) {
                    let threshold = accounts_file.settings.switch_threshold_percent;
                    if crate::strategy::is_account_depleted(active, threshold) {
                        return true;
                    }
                }
            }
        }
        !crate::switcher::detect_quota_blocked_user_threads_since(30).is_empty()
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

        loop {
            let _ = crate::logger::rotate_all_logs(
                crate::logger::DEFAULT_MAX_LOG_SIZE,
                crate::logger::DEFAULT_MAX_ARCHIVES,
            );
            if let Err(error) = DaemonTickService::run(true) {
                LogRedactionService::eprint_background(&format!("Error in daemon tick: {error}"));
                crate::logger::log("ERROR", "DAEMON", "Daemon tick failed");
            }

            let last_auth_mtime = auth_mtime();
            let interval_secs = load_accounts()
                .map(|accounts| accounts.settings.poll_interval_seconds.max(5))
                .unwrap_or(60);
            let sleep_start = Instant::now();
            let target_duration = Duration::from_secs(interval_secs);
            let mut watchdog_ticks = 0_u32;
            while sleep_start.elapsed() < target_duration {
                sleep(Duration::from_secs(1));
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
