use super::DesktopSessionBindingService;
use crate::distribution::mock_app_lifecycle::MockAppLifecycle;
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::distribution::{DesktopAppSession, WindowProcessIdentity};
use crate::storage::{load_accounts, read_active_auth_json};
use std::cell::Cell;

#[test]
fn direct_switch_binds_app_before_recovery_and_rejects_process_change() {
    let home = std::env::temp_dir().join(format!(
        "codex-desktop-bind-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
    ));
    std::fs::create_dir(&home).unwrap();
    let process = WindowProcessIdentity::new(9999, "123:456789").unwrap();
    let inspections = Cell::new(0);
    DesktopSessionBindingService::bind_then_recover_with(
        &home,
        "app-account",
        9999,
        || vec![9999],
        |_| {
            inspections.set(inspections.get() + 1);
            Ok(process.clone())
        },
        |_| {
            let marker = DesktopAppSession::load(&home.join("desktop-app-session.json")).unwrap();
            assert_eq!(marker.account_id, "app-account");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(inspections.get(), 2);
    let marker = DesktopAppSession::load(&home.join("desktop-app-session.json")).unwrap();
    assert_eq!(marker.account_id, "app-account");
    assert_eq!(marker.cli_account_id.as_deref(), Some("app-account"));
    assert_eq!(marker.process, Some(process.clone()));

    let original = std::fs::read(home.join("desktop-app-session.json")).unwrap();
    let changed = WindowProcessIdentity::new(9999, "124:456789").unwrap();
    let inspection = Cell::new(0);
    assert!(DesktopSessionBindingService::bind_then_recover_with(
        &home,
        "wrong-account",
        9999,
        || vec![9999],
        |_| {
            inspection.set(inspection.get() + 1);
            Ok(if inspection.get() == 1 {
                process.clone()
            } else {
                changed.clone()
            })
        },
        |_| panic!("recovery cannot start after process identity changes"),
    )
    .is_err());
    assert_eq!(
        std::fs::read(home.join("desktop-app-session.json")).unwrap(),
        original
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn retry_reconciles_cli_binding_after_partial_cli_commit() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("retry_cli_marker_reconcile");
    env.populate(
        vec![
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
                "old",
                None,
                "old@example.com",
                "plus",
                40.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "new",
                None,
                "new@example.com",
                "pro",
                80.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        Some("app"),
    );
    let marker_path = env.home().join("desktop-app-session.json");
    let original = DesktopAppSession::load(&marker_path).unwrap();
    let mut registry = load_accounts().unwrap();
    let mut auth = read_active_auth_json().unwrap();
    registry.active_account_id = Some("new@example.com:new".into());
    assert!(DesktopSessionBindingService::resolve_cli_account_id(&registry, &auth).is_err());
    auth.tokens.as_mut().unwrap().account_id = Some("new".into());
    assert_eq!(
        DesktopSessionBindingService::resolve_cli_account_id(&registry, &auth).unwrap(),
        "new@example.com:new"
    );
    let lifecycle = MockAppLifecycle::new(true);
    DesktopSessionBindingService::reconcile_with(
        env.home(),
        "new@example.com:new",
        || Ok("new@example.com:new".into()),
        &lifecycle,
    )
    .unwrap();
    let repaired = DesktopAppSession::load(&marker_path).unwrap();
    assert_eq!(repaired.account_id, original.account_id);
    assert_eq!(repaired.process, original.process);
    assert_eq!(
        repaired.cli_account_id.as_deref(),
        Some("new@example.com:new")
    );
    assert!(DesktopSessionBindingService::reconcile_with(
        env.home(),
        "old@example.com:old",
        || Ok("new@example.com:new".into()),
        &lifecycle
    )
    .is_err());
}
