use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_outcome::DistributionStatus;
use super::distribution_request::DistributionRequest;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_helper::{make_account, TestEnv};
use crate::storage::{load_accounts, read_active_auth_json};
use std::sync::atomic::Ordering;
use std::sync::{Arc, Mutex};

#[test]
fn desktop_account_is_bound_before_recovery_waits() {
    let env = TestEnv::new("desktop_marker_before_recovery");
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
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "cli",
                None,
                "cli@example.com",
                "pro",
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
    let observed = Arc::new(Mutex::new(None));
    let observed_in_recovery = observed.clone();
    let marker_path = env.home().join("desktop-app-session.json");
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_recovery(move || {
        *observed_in_recovery.lock().unwrap() =
            super::desktop_app_session::DesktopAppSession::load(&marker_path);
    });

    DistributionCoordinator::with_lifecycle(mock)
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();
    let session = observed.lock().unwrap().clone().unwrap();
    assert_eq!(session.account_id, "target@example.com:target");
    assert_eq!(
        session.cli_account_id.as_deref(),
        Some("target@example.com:target")
    );
    assert_eq!(
        session.process.as_ref().map(|process| process.pid),
        Some(9999)
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("cli@example.com:cli")
    );
    let final_session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(
        final_session.cli_account_id.as_deref(),
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
}

#[test]
fn stale_desktop_marker_cannot_drive_automatic_distribution() {
    let env = TestEnv::new("stale_desktop_marker_distribution");
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
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let old_process =
        super::window_restore_process_identity::ProcessIdentity::new(9999, "122:456789").unwrap();
    super::desktop_app_session::DesktopAppSession::bound(
        "old@example.com:old",
        "old@example.com:old",
        old_process,
    )
    .save(&env.home().join("desktop-app-session.json"))
    .unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"));

    assert!(result.is_err(), "a stale marker must fail closed");
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(env.log_content().contains("desktop_identity_unverified"));
}

#[test]
fn explicit_app_restart_repairs_stale_desktop_marker() {
    let env = TestEnv::new("explicit_stale_marker_repair");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                60.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let stale =
        super::window_restore_process_identity::ProcessIdentity::new(9999, "122:456789").unwrap();
    super::desktop_app_session::DesktopAppSession::bound(
        "old@example.com:old",
        "old@example.com:old",
        stale,
    )
    .save(&env.home().join("desktop-app-session.json"))
    .unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let request = DistributionRequest::user("explicit_app_repair")
        .with_preferred_app(Some("target".into()))
        .with_preferred_cli(Some("old".into()));
    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(request)
        .unwrap();
    assert_eq!(result.status, DistributionStatus::Success);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 1);
    let marker = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(marker.account_id, "target@example.com:target");
    assert_eq!(
        marker.cli_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert_eq!(marker.process.unwrap().birth_id, "123:456789");
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
}

#[test]
fn cli_only_distribution_preserves_depleted_desktop_binding_without_restart() {
    let env = TestEnv::new("cli_only_depleted_app");
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
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let marker_path = env.home().join("desktop-app-session.json");
    let original = super::desktop_app_session::DesktopAppSession::load(&marker_path).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let request = DistributionRequest::user("menu_cli_target")
        .with_preferred_app(Some("old".into()))
        .with_preferred_cli(Some("target".into()))
        .with_allow_restart(false);
    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(request)
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(
        outcome.target_app_id.as_deref(),
        Some("old@example.com:old")
    );
    assert_eq!(
        outcome.target_cli_id.as_deref(),
        Some("target@example.com:target")
    );
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    let updated = super::desktop_app_session::DesktopAppSession::load(&marker_path).unwrap();
    assert_eq!(updated.account_id, original.account_id);
    assert_eq!(updated.process, original.process);
    assert_eq!(
        updated.cli_account_id.as_deref(),
        Some("target@example.com:target")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("target@example.com:target")
    );
}

#[test]
fn automatic_cli_rotation_without_restart_preserves_desktop_binding() {
    let env = TestEnv::new("auto_cli_only");
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
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let marker_path = env.home().join("desktop-app-session.json");
    let original = super::desktop_app_session::DesktopAppSession::load(&marker_path).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted").with_allow_restart(false))
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    let updated = super::desktop_app_session::DesktopAppSession::load(&marker_path).unwrap();
    assert_eq!(updated.account_id, original.account_id);
    assert_eq!(updated.process, original.process);
    assert_eq!(
        updated.cli_account_id.as_deref(),
        Some("target@example.com:target")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("target@example.com:target")
    );
}

#[test]
fn stopped_desktop_distribution_does_not_claim_an_app_account() {
    let env = TestEnv::new("stopped_desktop_no_app_claim");
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
                "app",
                None,
                "app@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "cli",
                None,
                "cli@example.com",
                "pro",
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
    let mock = Arc::new(MockAppLifecycle::new(false));
    let outcome = DistributionCoordinator::with_lifecycle(mock)
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(outcome.target_app_id, None);
    assert_eq!(
        outcome.target_cli_id.as_deref(),
        Some("cli@example.com:cli")
    );
    assert!(!env.home().join("desktop-app-session.json").exists());
}

#[test]
fn failed_cli_registry_commit_after_relaunch_restores_original_cli_identity() {
    let env = TestEnv::new("cli_registry_rollback_after_relaunch");
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
                "app",
                None,
                "app@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "cli",
                None,
                "cli@example.com",
                "pro",
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
    let blocked_temp = env
        .home()
        .join(format!("accounts.{}.tmp.json", std::process::id()));
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_recovery(move || std::fs::create_dir(&blocked_temp).unwrap());
    let outcome = DistributionCoordinator::with_lifecycle(mock)
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::PartialSuccess);
    assert_eq!(
        outcome.target_cli_id.as_deref(),
        Some("old@example.com:old")
    );
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
    let marker = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(marker.account_id, "app@example.com:app");
    assert_eq!(
        marker.cli_account_id.as_deref(),
        Some("old@example.com:old")
    );
}

#[test]
fn app_retarget_without_restart_cannot_write_an_unbound_marker() {
    let env = TestEnv::new("no_restart_app_retarget");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.com",
                "plus",
                60.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let marker_path = env.home().join("desktop-app-session.json");
    let original = std::fs::read(&marker_path).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let request = DistributionRequest::user("no_restart")
        .with_preferred_app(Some("target".into()))
        .with_allow_restart(false);
    assert!(DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(request)
        .is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(marker_path).unwrap(), original);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
}

#[test]
fn unknown_cli_target_is_rejected_before_desktop_shutdown() {
    let env = TestEnv::new("invalid_cli_preflight");
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
                "app",
                None,
                "app@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let marker_path = env.home().join("desktop-app-session.json");
    let original_marker = std::fs::read(&marker_path).unwrap();
    let original_auth = read_active_auth_json().unwrap().tokens.unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let request = DistributionRequest::user("invalid_cli")
        .with_preferred_app(Some("app".into()))
        .with_preferred_cli(Some("missing-account".into()));
    assert!(DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(request)
        .is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(marker_path).unwrap(), original_marker);
    assert_eq!(
        read_active_auth_json().unwrap().tokens.unwrap(),
        original_auth
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
}

#[test]
fn desktop_identity_change_before_transaction_prevents_auth_mutation() {
    let env = TestEnv::new("desktop_changed_before_lock");
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
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.change_process_birth_after_first_inspection();
    assert!(DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
}

#[test]
fn failed_desktop_marker_save_prevents_recovery_dispatch() {
    let env = TestEnv::new("marker_save_failure_no_dispatch");
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
                "target",
                None,
                "target@example.com",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("old"),
    );
    let marker = env.home().join("desktop-app-session.json");
    let original = std::fs::read(&marker).unwrap();
    let blocked_temp = marker.with_extension(format!("{}.tmp", std::process::id()));
    std::fs::create_dir(&blocked_temp).unwrap();
    let mock = Arc::new(MockAppLifecycle::new(true));
    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::auto("quota_exhausted"))
        .unwrap();
    assert_eq!(result.status, DistributionStatus::PartialSuccess);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 0);
    assert_eq!(std::fs::read(marker).unwrap(), original);
}
