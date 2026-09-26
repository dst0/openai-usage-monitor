use super::*;
use crate::distribution::mock_app_lifecycle::MockAppLifecycle;
use crate::distribution::test_helper::{make_account, TestEnv};

#[test]
fn post_relaunch_commit_preserves_concurrent_registry_change() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("relaunch_registry_concurrency");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
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
                "next@example.test",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("next"),
        Some("next"),
    );
    let mut accounts = storage::load_accounts().unwrap();
    let target = accounts.active_account_id.clone().unwrap();
    DesktopAppSession::bound(
        target.clone(),
        target.clone(),
        crate::distribution::WindowProcessIdentity::new(9999, "test-birth").unwrap(),
    )
    .save(&env.home().join("desktop-app-session.json"))
    .unwrap();
    let lifecycle = MockAppLifecycle::new(true);

    DistributionAccountCommitService::commit_latest_desktop_auth_with_hook(
        &lifecycle,
        env.home(),
        &mut accounts,
        &target,
        || {
            storage::update_accounts_atomically(|registry| {
                registry.settings.auto_switch_enabled = false;
                registry
                    .accounts
                    .iter_mut()
                    .find(|account| account.account_id == "old")
                    .unwrap()
                    .priority = 77;
                Ok(())
            })?;
            Ok(())
        },
        || Ok(()),
    )
    .unwrap();

    let saved = storage::load_accounts().unwrap();
    assert!(!saved.settings.auto_switch_enabled);
    assert_eq!(
        saved
            .accounts
            .iter()
            .find(|account| account.account_id == "old")
            .unwrap()
            .priority,
        77
    );
}

#[test]
fn post_relaunch_commit_rejects_unknown_auth_change_after_registry_write() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("relaunch_auth_extension_change");
    env.populate(
        vec![make_account(
            "next",
            None,
            "next@example.test",
            "team",
            90.0,
            None,
            0,
            None,
            None,
        )],
        Some("next"),
        Some("next"),
    );
    let mut accounts = storage::load_accounts().unwrap();
    let target = accounts.active_account_id.clone().unwrap();
    DesktopAppSession::bound(
        target.clone(),
        target.clone(),
        crate::distribution::WindowProcessIdentity::new(9999, "test-birth").unwrap(),
    )
    .save(&env.home().join("desktop-app-session.json"))
    .unwrap();
    let lifecycle = MockAppLifecycle::new(true);

    let result = DistributionAccountCommitService::commit_latest_desktop_auth_with_hook(
        &lifecycle,
        env.home(),
        &mut accounts,
        &target,
        || Ok(()),
        || {
            let mut auth = storage::read_active_auth_json()?;
            auth.extra.insert(
                "desktop_extension".into(),
                serde_json::json!({"revision": 2}),
            );
            storage::write_active_auth_json(&auth)
        },
    );

    assert!(
        result.is_err(),
        "unverified Desktop auth change cannot report success"
    );
}

#[test]
fn unknown_auth_field_change_blocks_readback_and_rollback() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("unknown_auth_change");
    env.populate(
        vec![make_account(
            "old",
            None,
            "old@example.test",
            "plus",
            0.0,
            None,
            0,
            None,
            None,
        )],
        Some("old"),
        None,
    );
    let lifecycle = MockAppLifecycle::new(false);
    let committed = storage::read_active_auth_json().unwrap();
    let mut previous = committed.clone();
    previous.tokens.as_mut().unwrap().refresh_token = Some("prior-test-token".into());
    let mut changed = committed.clone();
    changed.extra.insert(
        "desktop_extension".into(),
        serde_json::json!({"revision": 2}),
    );
    storage::write_active_auth_json(&changed).unwrap();

    assert!(DistributionAccountCommitService::verify_offline_auth(&lifecycle, &committed).is_err());
    assert!(
        DistributionAccountCommitService::restore_when_desktop_stopped(
            &lifecycle, &previous, &committed,
        )
        .is_err()
    );
    assert_eq!(storage::read_active_auth_json().unwrap(), changed);
}

#[test]
fn desktop_switch_does_not_carry_previous_api_key_to_next_account() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("desktop_switch_previous_api_key");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
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
                "next@example.test",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        None,
    );
    let mut previous = storage::read_active_auth_json().unwrap();
    previous.openai_api_key = Some("synthetic-prior-account-key".into());
    storage::write_active_auth_json(&previous).unwrap();
    let target = storage::load_accounts()
        .unwrap()
        .accounts
        .into_iter()
        .find(|account| account.account_id == "next")
        .unwrap();

    let (_, switched) =
        DistributionAccountCommitService::apply_auth_tokens(&MockAppLifecycle::new(false), &target)
            .unwrap();

    assert!(switched.openai_api_key.is_none());
    assert!(storage::read_active_auth_json()
        .unwrap()
        .openai_api_key
        .is_none());
}

#[test]
fn desktop_switch_rejects_concurrent_auth_replacement() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("desktop_switch_concurrent_auth");
    env.populate(
        vec![
            make_account(
                "old",
                None,
                "old@example.test",
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
                "next@example.test",
                "team",
                90.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("old"),
        None,
    );
    let target = storage::load_accounts()
        .unwrap()
        .accounts
        .into_iter()
        .find(|account| account.account_id == "next")
        .unwrap();
    let mut external = storage::read_active_auth_json().unwrap();
    external.tokens.as_mut().unwrap().refresh_token = Some("synthetic-external-rotation".into());

    let result = DistributionAccountCommitService::apply_auth_tokens_with_hook(
        &MockAppLifecycle::new(false),
        &target,
        || storage::write_active_auth_json(&external).unwrap(),
    );

    assert!(result.is_err());
    assert_eq!(storage::read_active_auth_json().unwrap(), external);
}

#[test]
fn rollback_rejects_concurrent_auth_replacement() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("desktop_rollback_concurrent_auth");
    env.populate(
        vec![make_account(
            "old",
            None,
            "old@example.test",
            "plus",
            0.0,
            None,
            0,
            None,
            None,
        )],
        Some("old"),
        None,
    );
    let committed = storage::read_active_auth_json().unwrap();
    let mut previous = committed.clone();
    previous.tokens.as_mut().unwrap().refresh_token = Some("synthetic-previous-token".into());
    let mut external = committed.clone();
    external.tokens.as_mut().unwrap().refresh_token = Some("synthetic-external-rotation".into());

    let result = DistributionAccountCommitService::restore_when_desktop_stopped_with_hook(
        &MockAppLifecycle::new(false),
        &previous,
        &committed,
        || storage::write_active_auth_json(&external).unwrap(),
    );

    assert!(result.is_err());
    assert_eq!(storage::read_active_auth_json().unwrap(), external);
}
