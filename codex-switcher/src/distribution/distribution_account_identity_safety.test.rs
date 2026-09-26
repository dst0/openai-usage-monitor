use super::distribution_audit_logger::DistributionAuditLogger;
use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_decision_service::DistributionDecisionService;
use super::distribution_outcome::DistributionStatus;
use super::distribution_request::DistributionRequest;
use super::distribution_transaction_service::DistributionTransactionService;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_account_spec::TestAccountSpec;
use super::test_helper::TestEnv;
use crate::storage::{load_accounts, read_active_auth_json, save_accounts, write_active_auth_json};
use base64::Engine;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[test]
fn closed_desktop_distribution_uses_one_shared_target_without_relaunch() {
    let env = TestEnv::new("cli_only_registry_write_failure");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let mock = Arc::new(MockAppLifecycle::new(false));
    let outcome = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(
            DistributionRequest::user("switch cli")
                .with_preferred_cli(Some("next".into()))
                .with_allow_restart(false),
        )
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("next@example.com:next")
    );
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("next")
    );
    let marker = super::desktop_app_session::DesktopAppSession::load_checked(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap()
    .unwrap();
    assert_eq!(marker.account_id, "next@example.com:next");
    assert!(marker.process.is_none());
    assert!(!env.home().join("distribution-journal.json").exists());
}

#[test]
fn mismatched_existing_app_cli_binding_blocks_before_shutdown() {
    let env = TestEnv::new("checkpoint_cli_restore_failure");
    env.populate(
        vec![
            TestAccountSpec {
                id: "app",
                email: "app@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "cli",
                email: "cli@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("cli"),
        Some("app"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));
    let marker_path = env.home().join("desktop-app-session.json");
    let original_marker = std::fs::read(&marker_path).unwrap();
    let result = DistributionCoordinator::with_lifecycle(mock.clone()).execute(
        DistributionRequest::user("split prior state").with_preferred_app(Some("next".into())),
    );
    assert!(result.is_err());
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("cli")
    );
    assert_eq!(std::fs::read(&marker_path).unwrap(), original_marker);
}

#[test]
fn stale_cli_plan_cannot_overwrite_a_switch_completed_before_operation_lock() {
    let env = TestEnv::new("stale_cli_plan");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "other",
                email: "other@example.com",
                plan: "plus",
                sprint_pct: 70.0,
                ..TestAccountSpec::default()
            }
            .build(),
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
    let env = TestEnv::new("cli_auth_mismatch");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
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
fn offline_switch_preserves_previous_accounts_rotated_refresh_token() {
    let env = TestEnv::new("offline_previous_token_rotation");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.test",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.test",
                plan: "team",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    let mut rotated = read_active_auth_json().unwrap();
    let claims =
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(r#"{"email":"old@example.test"}"#);
    let tokens = rotated.tokens.as_mut().unwrap();
    tokens.access_token = format!("synthetic.{claims}.signature");
    tokens.refresh_token = Some("synthetic-offline-rotation".into());
    write_active_auth_json(&rotated).unwrap();

    let outcome = DistributionCoordinator::with_lifecycle(Arc::new(MockAppLifecycle::new(false)))
        .execute(
            DistributionRequest::user("offline switch")
                .with_preferred_app(Some("next".into()))
                .with_preferred_cli(Some("next".into())),
        )
        .unwrap();
    assert_eq!(outcome.status, DistributionStatus::Success);
    let saved = load_accounts().unwrap();
    let old = saved
        .accounts
        .iter()
        .find(|account| account.account_id == "old")
        .unwrap();
    assert_eq!(
        old.tokens.refresh_token.as_deref(),
        Some("synthetic-offline-rotation")
    );
}

#[test]
fn journal_replacement_after_shutdown_relaunches_previous_desktop() {
    let env = TestEnv::new("journal_failure_after_stop");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        Some("old"),
    );
    let home = env.home().to_path_buf();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_stop(move || {
        let path = home.join("distribution-journal.json");
        std::fs::rename(&path, home.join("distribution-journal.saved.json")).unwrap();
        std::fs::create_dir(&path).unwrap();
    });

    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::user("switch app").with_preferred_app(Some("next".into())));
    assert!(result.is_err());
    assert!(mock.running.load(Ordering::SeqCst));
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 1);
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
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "old@example.com:old");
    assert_eq!(
        session.cli_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert_eq!(session.process.unwrap().birth_id, "123:456789");
    assert!(env.home().join("distribution-journal.json").is_dir());
    assert!(env.home().join("distribution-journal.saved.json").is_file());
}

#[test]
fn explicit_split_targets_preserve_existing_desktop_state() {
    let env = TestEnv::new("distinct_post_stop_recovery");
    env.populate(
        vec![
            TestAccountSpec {
                id: "app",
                email: "app@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "cli",
                email: "cli@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("cli"),
        Some("cli"),
    );
    let mock = Arc::new(MockAppLifecycle::new(true));

    let result = DistributionCoordinator::with_lifecycle(mock.clone()).execute(
        DistributionRequest::user("switch app")
            .with_preferred_app(Some("next".into()))
            .with_preferred_cli(Some("cli".into())),
    );
    assert!(result.is_err());
    assert!(mock.running.load(Ordering::SeqCst));
    assert_eq!(mock.stop_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 0);
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "cli@example.com:cli");
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
fn changed_auth_after_stop_blocks_previous_relaunch_and_recovery() {
    let env = TestEnv::new("app_auth_staging_failure");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.com",
                plan: "plus",
                sprint_pct: 50.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "plus",
                sprint_pct: 80.0,
                ..TestAccountSpec::default()
            }
            .build(),
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
    let mut changed_auth = read_active_auth_json().unwrap();
    changed_auth.tokens = Some(next.tokens);
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_stop(move || {
        write_active_auth_json(&changed_auth).unwrap();
    });

    let result = DistributionCoordinator::with_lifecycle(mock.clone())
        .execute(DistributionRequest::user("switch app").with_preferred_app(Some("next".into())));
    assert!(result.is_err());
    assert!(!mock.running.load(Ordering::SeqCst));
    assert_eq!(mock.launch_calls.load(Ordering::SeqCst), 0);
    assert_eq!(mock.recovery_calls.load(Ordering::SeqCst), 0);
    assert_eq!(
        read_active_auth_json()
            .unwrap()
            .tokens
            .unwrap()
            .account_id
            .as_deref(),
        Some("next")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("old@example.com:old")
    );
    assert!(env.home().join("distribution-journal.json").exists());
}

#[test]
fn journal_replaced_during_recovery_reports_partial_without_second_restart() {
    let env = TestEnv::new("cli_journal_failure_after_relaunch");
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                email: "old@example.com",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                email: "next@example.com",
                plan: "team",
                sprint_pct: 80.0,
                credits: 1,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        Some("old"),
    );
    let home = env.home().to_path_buf();
    let mock = Arc::new(MockAppLifecycle::new(true));
    mock.observe_recovery(move || {
        let path = home.join("distribution-journal.json");
        std::fs::rename(&path, home.join("distribution-journal.saved.json")).unwrap();
        std::fs::create_dir(&path).unwrap();
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
        Some("next")
    );
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some("next@example.com:next")
    );
    let session = super::desktop_app_session::DesktopAppSession::load(
        &env.home().join("desktop-app-session.json"),
    )
    .unwrap();
    assert_eq!(session.account_id, "next@example.com:next");
    assert_eq!(
        session.cli_account_id.as_deref(),
        Some("next@example.com:next")
    );
    assert!(env.home().join("distribution-journal.json").is_dir());
    assert!(env.home().join("distribution-journal.saved.json").is_file());
}
