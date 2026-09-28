use super::DaemonLoopService;
use crate::distribution::automatic_distribution_backoff::AutomaticDistributionBackoff;
use crate::models::{AccountConfig, AccountsFile, Settings};
use std::time::{Duration, Instant};
use std::{cell::Cell, os::unix::fs::PermissionsExt};

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
    let _home = crate::storage::test_codex_home::TestCodexHome::new("watchdog-off");
    let backoff = AutomaticDistributionBackoff::default();
    let mut accounts = accounts_with_quota(0.0, 100.0, 0);
    assert!(!accounts.settings.auto_switch_enabled);
    crate::storage::save_accounts(&accounts).unwrap();

    assert!(!DaemonLoopService::watchdog_needs_immediate_check(
        &backoff,
        Instant::now()
    ));

    accounts.settings.auto_switch_enabled = true;
    crate::storage::save_accounts(&accounts).unwrap();
    assert!(DaemonLoopService::watchdog_needs_immediate_check(
        &backoff,
        Instant::now()
    ));
}

#[test]
fn disabled_watchdog_does_not_probe_quota_threads_and_enabled_watchdog_does() {
    let accounts = accounts_with_quota(50.0, 100.0, 0);
    let backoff = AutomaticDistributionBackoff::default();
    let scans = Cell::new(0);
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        Instant::now(),
        Instant::now(),
        || {
            scans.set(scans.get() + 1);
            true
        },
    );
    assert!(!found);
    assert_eq!(scans.get(), 0);

    let mut enabled = accounts;
    enabled.settings.auto_switch_enabled = true;
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(
        &enabled,
        &backoff,
        Instant::now(),
        Instant::now(),
        || {
            scans.set(scans.get() + 1);
            true
        },
    );
    assert!(found);
    assert_eq!(scans.get(), 1);
}

#[test]
fn weekly_reset_only_preserves_recent_task_probe() {
    let mut accounts = accounts_with_quota(50.0, 0.0, 1);
    let backoff = AutomaticDistributionBackoff::default();
    accounts.settings.auto_reset_weekly_enabled = true;
    let scans = Cell::new(0);
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        Instant::now(),
        Instant::now(),
        || {
            scans.set(scans.get() + 1);
            true
        },
    );
    assert!(found);
    assert_eq!(scans.get(), 1);

    // The daemon tick can select the first configured account when its active
    // pointer is absent; keep the existing recent-task wakeup in that case.
    accounts.active_account_id = None;
    let found = DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        Instant::now(),
        Instant::now(),
        || {
            scans.set(scans.get() + 1);
            true
        },
    );
    assert!(found);
    assert_eq!(scans.get(), 2);
}

#[test]
fn invalid_registry_does_not_start_immediate_quota_scan() {
    let home = crate::storage::test_codex_home::TestCodexHome::new("watchdog-off");
    let backoff = AutomaticDistributionBackoff::default();
    let path = home.path().join("accounts.json");
    std::fs::write(&path, b"invalid registry").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(!DaemonLoopService::watchdog_needs_immediate_check(
        &backoff,
        Instant::now()
    ));
}

#[test]
fn held_automatic_plan_does_not_wake_full_quota_refresh_every_two_seconds() {
    let mut accounts = accounts_with_quota(0.0, 100.0, 0);
    accounts.settings.auto_switch_enabled = true;
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    assert_eq!(
        backoff.record_pre_signal_failure("plan", "WINDOW_ACCESS_FAILED", "denied", now),
        None
    );
    assert!(backoff
        .record_pre_signal_failure("plan", "WINDOW_ACCESS_FAILED", "denied", now)
        .is_some());
    let scans = Cell::new(0);
    let recent = || {
        scans.set(scans.get() + 1);
        true
    };
    assert!(!DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        now + Duration::from_secs(1),
        now,
        recent,
    ));
    assert_eq!(scans.get(), 0);
    accounts.settings.auto_reset_weekly_enabled = true;
    assert!(!DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        now + Duration::from_secs(2),
        now,
        || {
            scans.set(scans.get() + 1);
            true
        },
    ));
    assert_eq!(scans.get(), 0);
    assert!(DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        now + Duration::from_secs(30),
        now,
        || {
            scans.set(scans.get() + 1);
            true
        },
    ));
    assert_eq!(scans.get(), 1);
    assert!(!DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        now + Duration::from_secs(31),
        now + Duration::from_secs(30),
        || {
            scans.set(scans.get() + 1);
            true
        },
    ));
    assert_eq!(scans.get(), 1);
    assert!(DaemonLoopService::watchdog_needs_immediate_check_with(
        &accounts,
        &backoff,
        now + Duration::from_secs(5 * 60 + 1),
        now + Duration::from_secs(30),
        || false,
    ));
}
