use super::DaemonLoopService;
use crate::models::{AccountConfig, AccountsFile, Settings};
use std::{cell::Cell, ffi::OsString, os::unix::fs::PermissionsExt, path::PathBuf};

struct TestHome {
    path: PathBuf,
    previous: Option<OsString>,
}

impl TestHome {
    fn new() -> Self {
        let path = std::env::temp_dir().join(format!(
            "codex-watchdog-off-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir(&path).unwrap();
        let previous = std::env::var_os("CODEX_HOME");
        std::env::set_var("CODEX_HOME", &path);
        Self { path, previous }
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        if let Some(value) = &self.previous {
            std::env::set_var("CODEX_HOME", value);
        } else {
            std::env::remove_var("CODEX_HOME");
        }
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn accounts_with_quota(primary: f64, weekly: f64, credits: u32) -> AccountsFile {
    let account: AccountConfig = serde_json::from_value(serde_json::json!({
        "id": "synthetic-account",
        "email": "synthetic@example.invalid",
        "account_id": "synthetic-workspace",
        "tokens": {"access_token": "synthetic-access"},
        "last_primary_percentage": primary,
        "last_weekly_percentage": weekly,
        "last_credits": credits
    }))
    .unwrap();
    AccountsFile {
        active_account_id: Some(account.id.clone()),
        settings: Settings::default(),
        accounts: vec![account],
    }
}

#[test]
fn disabled_auto_switch_does_not_wake_watchdog_for_depleted_account() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let _home = TestHome::new();
    let mut accounts = accounts_with_quota(0.0, 100.0, 0);
    assert!(!accounts.settings.auto_switch_enabled);
    crate::storage::save_accounts(&accounts).unwrap();

    assert!(!DaemonLoopService::watchdog_needs_immediate_check());

    accounts.settings.auto_switch_enabled = true;
    crate::storage::save_accounts(&accounts).unwrap();
    assert!(DaemonLoopService::watchdog_needs_immediate_check());
}

#[test]
fn disabled_watchdog_does_not_probe_quota_threads_and_enabled_watchdog_does() {
    let accounts = accounts_with_quota(50.0, 100.0, 0);
    let scans = Cell::new(0);
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(&accounts, || {
        scans.set(scans.get() + 1);
        true
    });
    assert!(!found);
    assert_eq!(scans.get(), 0);

    let mut enabled = accounts;
    enabled.settings.auto_switch_enabled = true;
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(&enabled, || {
        scans.set(scans.get() + 1);
        true
    });
    assert!(found);
    assert_eq!(scans.get(), 1);
}

#[test]
fn weekly_reset_only_preserves_recent_task_probe() {
    let mut accounts = accounts_with_quota(50.0, 0.0, 1);
    accounts.settings.auto_reset_weekly_enabled = true;
    let scans = Cell::new(0);
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(&accounts, || {
        scans.set(scans.get() + 1);
        true
    });
    assert!(found);
    assert_eq!(scans.get(), 1);

    // The daemon tick can select the first configured account when its active
    // pointer is absent; keep the existing recent-task wakeup in that case.
    accounts.active_account_id = None;
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(&accounts, || {
        scans.set(scans.get() + 1);
        true
    });
    assert!(found);
    assert_eq!(scans.get(), 2);
}

#[test]
fn invalid_registry_does_not_start_immediate_quota_scan() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let home = TestHome::new();
    let path = home.path.join("accounts.json");
    std::fs::write(&path, b"invalid registry").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!DaemonLoopService::watchdog_needs_immediate_check());
}
