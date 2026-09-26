use super::DesktopSessionBindingService;
use crate::distribution::mock_app_lifecycle::MockAppLifecycle;
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::distribution::AppLifecycle;
use crate::distribution::{DesktopAppSession, WindowProcessIdentity};
use crate::storage::{load_accounts, read_active_auth_json, save_accounts, write_active_auth_json};
use base64::Engine;
use std::cell::Cell;
use std::sync::atomic::Ordering;

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
        || Ok("app-account".into()),
        |_| {
            let marker = DesktopAppSession::load(&home.join("desktop-app-session.json")).unwrap();
            assert_eq!(marker.account_id, "app-account");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(inspections.get(), 3);
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
        || Ok("wrong-account".into()),
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
fn retry_refuses_to_bind_cli_auth_to_a_different_desktop_account() {
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
    assert!(DesktopSessionBindingService::resolve_cli_account_id(&registry, &auth).is_err());
    auth.tokens = Some(
        registry
            .accounts
            .iter()
            .find(|account| account.account_id == "new")
            .unwrap()
            .tokens
            .clone(),
    );
    assert_eq!(
        DesktopSessionBindingService::resolve_cli_account_id(&registry, &auth).unwrap(),
        "new@example.com:new"
    );
    let lifecycle = MockAppLifecycle::new(true);
    assert!(DesktopSessionBindingService::reconcile_with(
        env.home(),
        "new@example.com:new",
        || Ok("new@example.com:new".into()),
        &lifecycle,
    )
    .is_err());
    assert_eq!(DesktopAppSession::load(&marker_path).unwrap(), original);
    assert!(DesktopSessionBindingService::reconcile_with(
        env.home(),
        "old@example.com:old",
        || Ok("new@example.com:new".into()),
        &lifecycle
    )
    .is_err());
    assert_eq!(DesktopAppSession::load(&marker_path).unwrap(), original);
}

#[test]
fn same_email_cannot_relabel_another_accounts_saved_tokens() {
    let env = TestEnv::new("same_email_different_provider");
    let mut old = make_account(
        "old",
        None,
        "shared@example.test",
        "plus",
        40.0,
        None,
        0,
        None,
        None,
    );
    let claims = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .encode(r#"{"email":"shared@example.test"}"#);
    old.tokens.access_token = format!("synthetic.{claims}.signature");
    let new = make_account(
        "new",
        None,
        "shared@example.test",
        "team",
        80.0,
        None,
        0,
        None,
        None,
    );
    env.populate(vec![old, new], Some("old"), Some("old"));
    let mut registry = load_accounts().unwrap();
    let mut auth = read_active_auth_json().unwrap();
    registry.active_account_id = Some("shared@example.test:new".into());
    auth.tokens.as_mut().unwrap().account_id = Some("new".into());

    assert!(DesktopSessionBindingService::resolve_cli_account_id(&registry, &auth).is_err());
}

#[test]
fn direct_relaunch_restoring_previous_auth_blocks_recovery_dispatch() {
    let env = TestEnv::new("direct_relaunch_previous_auth");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
                "plus",
                40.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.test",
                "team",
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
    let old_auth = read_active_auth_json().unwrap();
    let mut registry = load_accounts().unwrap();
    let target = registry
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap()
        .clone();
    registry.active_account_id = Some(target.id.clone());
    save_accounts(&registry).unwrap();
    let mut target_auth = old_auth.clone();
    target_auth.tokens = Some(target.tokens);
    write_active_auth_json(&target_auth).unwrap();
    let lifecycle = MockAppLifecycle::new(false);
    lifecycle.observe_launch(move || write_active_auth_json(&old_auth).unwrap());
    let pids = lifecycle.launch_app().unwrap();

    let result = DesktopSessionBindingService::bind_then_recover_with(
        env.home(),
        &target.id,
        pids[0],
        || pids.clone(),
        |pid| lifecycle.inspect_process(pid),
        DesktopSessionBindingService::verified_cli_account_id,
        |_| lifecycle.recover_threads(&[]),
    );

    assert!(result.is_err());
    assert_eq!(lifecycle.recovery_calls.load(Ordering::SeqCst), 0);
    assert!(
        DesktopAppSession::load_checked(&env.home().join("desktop-app-session.json"))
            .unwrap()
            .is_none()
    );
}

#[test]
fn auth_change_after_initial_binding_check_blocks_recovery_dispatch() {
    let env = TestEnv::new("direct_banner_auth_change");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
                "plus",
                40.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "next",
                None,
                "next@example.test",
                "team",
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
    let old_auth = read_active_auth_json().unwrap();
    let mut registry = load_accounts().unwrap();
    let target = registry
        .accounts
        .iter()
        .find(|account| account.account_id == "next")
        .unwrap()
        .clone();
    registry.active_account_id = Some(target.id.clone());
    save_accounts(&registry).unwrap();
    let mut target_auth = old_auth.clone();
    target_auth.tokens = Some(target.tokens);
    write_active_auth_json(&target_auth).unwrap();
    let lifecycle = MockAppLifecycle::new(false);
    let pids = lifecycle.launch_app().unwrap();

    let result = DesktopSessionBindingService::bind_then_recover_with(
        env.home(),
        &target.id,
        pids[0],
        || pids.clone(),
        |pid| lifecycle.inspect_process(pid),
        DesktopSessionBindingService::verified_cli_account_id,
        |process| {
            // The first check passed. A banner/window operation now restores
            // the previous account before the owner IPC dispatch boundary.
            write_active_auth_json(&old_auth)?;
            DesktopSessionBindingService::verify_bound_with(
                env.home(),
                &target.id,
                process,
                DesktopSessionBindingService::verified_cli_account_id,
                || pids.clone(),
                |pid| lifecycle.inspect_process(pid),
            )?;
            lifecycle.recover_threads(&[])
        },
    );

    assert!(result.is_err());
    assert_eq!(lifecycle.recovery_calls.load(Ordering::SeqCst), 0);
    assert!(
        DesktopAppSession::load_checked(&env.home().join("desktop-app-session.json"))
            .unwrap()
            .is_none()
    );
}
