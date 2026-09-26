use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_decision_service::DistributionDecisionService;
use super::distribution_outcome::DistributionStatus;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_service::DistributionTransactionService;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_helper::{make_account, TestEnv};
use crate::storage::{load_accounts, read_active_auth_json, save_accounts, write_active_auth_json};
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[test]
fn cli_only_registry_write_failure_is_not_reported_as_distributed() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("cli_only_registry_write_failure");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        None,
    );
    let temp_path = env
        .home()
        .join(format!("accounts.{}.tmp.json", std::process::id()));
    std::fs::create_dir(&temp_path).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(false));
    let result = DistributionCoordinator::with_lifecycle(mock.clone()).execute(
        DistributionRequest::user("switch cli")
            .with_preferred_cli(Some("next".into()))
            .with_allow_restart(false),
    );
    assert!(result.is_err(), "a rolled-back CLI-only commit must fail");
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("old")
    );
    assert!(!env.home().join("distribution-journal.json").exists());
}

#[test]
fn failed_cli_restore_keeps_checkpoint_relaunch_binding_on_staged_app_auth() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("checkpoint_cli_restore_failure");
    env.populate(
        vec![
            make_account(
                "app",
                None,
                "app@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "cli",
                None,
                "cli@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("cli"),
        Some("app"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.set_capture_mode(super::window_capture_mode::WindowCaptureMode::Absent);
    *mock.corrupt_manifest_after_stop.lock().unwrap() =
        Some(env.home().join("desktop-recovery.json"));
    let home = env.home().to_path_buf();
    mock.observe_launch(move || {
        std::fs::create_dir(home.join(format!("auth.{}.tmp.json", std::process::id()))).unwrap();
    });
    let result = DistributionCoordinator::with_lifecycle(mock.clone()).execute(
        DistributionRequest::user("checkpoint failure").with_preferred_app(Some("next".into())),
    );
    assert!(result.is_err());
    assert!(result
        .err()
        .unwrap()
        .contains("Original CLI authentication could not be restored"));
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("app")
    );
    let marker = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(marker.account_id, "app@example.com:app");
    assert_eq!(
        marker.cli_account_id.as_deref(),
        Some("app@example.com:app")
    );
}

#[test]
fn stale_cli_plan_cannot_overwrite_a_switch_completed_before_operation_lock() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("stale_cli_plan");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "other",
                None,
                "other@example.com",
                "plus",
                70.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let initial = load_accounts().unwrap();
    let old_id = initial
        .accounts
        .iter()
        .find(|account| account.account_id == "old")
        .unwrap()
        .id
        .clone();
    let other = initial
        .accounts
        .iter()
        .find(|account| account.account_id == "other")
        .unwrap()
        .clone();
    let request = DistributionRequest::user("switch cli")
        .with_preferred_cli(Some("next".into()))
        .with_allow_restart(false);
    let plan = DistributionDecisionService::new().evaluate(
        &initial,
        Some(&old_id),
        initial.active_account_id.as_deref(),
        true,
        &request,
    );
    assert!(plan.cli_switch_needed);
    assert!(!plan.restart_required);

    let mut live = initial.clone();
    live.active_account_id = Some(other.id.clone());
    save_accounts(&live).unwrap();
    let mut auth = read_active_auth_json().unwrap();
    auth.tokens = Some(other.tokens.clone());
    write_active_auth_json(&auth).unwrap();

    let mock = Arc::new(MockAppLifecycle::new(true));
    let result = DistributionTransactionService::with_lifecycle(
        DistributionAuditLogger::default(),
        mock.clone(),
    )
    .execute("op_stale_cli", &plan, &request, initial);
    assert!(result.is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some(other.id.as_str())
    );
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("other")
    );
    assert!(!env.home().join("distribution-journal.json").exists());
}

#[test]
fn distribution_rejects_cli_auth_that_disagrees_with_registry() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("cli_auth_mismatch");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let next = load_accounts()
        .unwrap()
        .accounts
        .into_iter()
        .find(|account| account.account_id == "next")
        .unwrap();
    let mut auth = read_active_auth_json().unwrap();
    auth.tokens = Some(next.tokens);
    write_active_auth_json(&auth).unwrap();

    let mock = Arc::new(MockAppLifecycle::new(true));
    let result = DistributionCoordinator::with_lifecycle(mock.clone()).execute(
        DistributionRequest::user("switch cli")
            .with_preferred_cli(Some("next".into()))
            .with_allow_restart(false),
    );
    assert!(result.is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(!env.home().join("distribution-journal.json").exists());
}

#[test]
fn journal_write_failure_after_shutdown_relaunches_previous_desktop() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("journal_failure_after_stop");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let home = env.home().to_path_buf();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_stop(move || {
        std::fs::create_dir_all(
            home.join(format!("distribution-journal.{}.tmp", std::process::id())),
        )
        .unwrap();
    });

    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::user("switch app").with_preferred_app(Some("next".into())));
    assert!(result.is_err());
    assert!(mock.running.load(Ordering::SeqCst));
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("old")
    );
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "old@example.com:old");
    assert_eq!(
        session.cli_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(!env.home().join("distribution-journal.json").exists());
}

#[test]
fn post_stop_recovery_preserves_distinct_previous_app_and_cli_accounts() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("distinct_post_stop_recovery");
    env.populate(
        vec![
            make_account(
                "app",
                None,
                "app@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "cli",
                None,
                "cli@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("cli"),
        Some("app"),
    );
    let home = env.home().to_path_buf();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_stop(move || {
        std::fs::create_dir_all(
            home.join(format!("distribution-journal.{}.tmp", std::process::id())),
        )
        .unwrap();
    });

    let result = DistributionCoordinator::with_lifecycle(mock.clone()).execute(
        DistributionRequest::user("switch app")
            .with_preferred_app(Some("next".into()))
            .with_preferred_cli(Some("cli".into())),
    );
    assert!(result.is_err());
    assert!(mock.running.load(Ordering::SeqCst));
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "app@example.com:app");
    assert_eq!(
        session.cli_account_id.as_deref(),
        Some("cli@example.com:cli")
    );
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("cli")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("cli@example.com:cli")
    );
}

#[test]
fn failed_app_auth_staging_after_stop_still_relaunches_desktop() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("app_auth_staging_failure");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                50.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "plus",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let home = env.home().to_path_buf();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_stop(move || {
        std::fs::create_dir_all(home.join(format!("auth.{}.tmp.json", std::process::id())))
            .unwrap();
    });

    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::user("switch app").with_preferred_app(Some("next".into())));
    assert!(result.is_err());
    assert!(mock.running.load(Ordering::SeqCst));
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("old")
    );
}

#[test]
fn cli_journal_failure_after_relaunch_restores_cli_without_restarting_app_again() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("cli_journal_failure_after_relaunch");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                0.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.com",
                "team",
                80.0,
                None,
                1,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let home = env.home().to_path_buf();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_recovery(move || {
        std::fs::create_dir_all(
            home.join(format!("distribution-journal.{}.tmp", std::process::id())),
        )
        .unwrap();
    });

    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::PartialSuccess);
    assert!(mock.running.load(Ordering::SeqCst));
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("old")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "next@example.com:next");
    assert_eq!(
        session.cli_account_id.as_deref(),
        Some("old@example.com:old")
    );
}
