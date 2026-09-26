use super::AccountSwitchCommitService;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::storage::{
    load_accounts, read_active_auth_json, update_accounts_atomically, write_active_auth_json,
};

fn fixture(label: &str) -> TestEnv {
    let env = TestEnv::new(label);
    env.populate(
        vec![
            TestAccountSpec {
                id: "old",
                name: Some("old"),
                email: "old@example.test",
                plan: "plus",
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "next",
                name: Some("next"),
                email: "next@example.test",
                plan: "team",
                sprint_pct: 90.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("old"),
        None,
    );
    env
}

fn finish_fixture(env: TestEnv, prior_home: Option<std::ffi::OsString>) {
    drop(env);
    if let Some(prior_home) = prior_home {
        std::env::set_var("CODEX_HOME", prior_home);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
}

#[test]
fn direct_switch_commit_preserves_interleaved_settings_and_other_tokens() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_commit_interleaved_settings");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();

    AccountSwitchCommitService::commit_with_hook(
        &selected,
        initial.active_account_id.as_deref(),
        || {
            update_accounts_atomically(|latest| {
                latest.settings.auto_switch_enabled = false;
                latest.settings.auto_switch_business_priority = false;
                latest.accounts[0].tokens.access_token = "newer-synthetic-token".into();
                latest.accounts[0].name = Some("newer nickname".into());
                latest.accounts[1].name = Some("renamed target".into());
                latest.accounts[1].priority = 7;
                Ok(())
            })?;
            Ok(())
        },
    )
    .unwrap();

    let saved = load_accounts().unwrap();
    assert_eq!(
        saved.active_account_id.as_deref(),
        Some(selected.id.as_str())
    );
    assert!(!saved.settings.auto_switch_enabled);
    assert!(!saved.settings.auto_switch_business_priority);
    assert_eq!(
        saved.accounts[0].tokens.access_token,
        "newer-synthetic-token"
    );
    assert_eq!(saved.accounts[0].name.as_deref(), Some("newer nickname"));
    assert_eq!(saved.accounts[1].name.as_deref(), Some("renamed target"));
    assert_eq!(saved.accounts[1].priority, 7);
    finish_fixture(env, prior_home);
}

#[test]
fn direct_switch_commit_rejects_interleaved_target_rotation() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_commit_rotated_target");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();

    let result = AccountSwitchCommitService::commit_with_hook(
        &selected,
        initial.active_account_id.as_deref(),
        || {
            update_accounts_atomically(|latest| {
                latest.accounts[1].tokens.refresh_token = Some("rotated-synthetic-refresh".into());
                Ok(())
            })?;
            Ok(())
        },
    );

    assert!(result.is_err());
    let saved = load_accounts().unwrap();
    assert_eq!(saved.active_account_id, initial.active_account_id);
    assert_eq!(
        saved.accounts[1].tokens.refresh_token.as_deref(),
        Some("rotated-synthetic-refresh")
    );
    finish_fixture(env, prior_home);
}

#[test]
fn direct_switch_commit_rejects_removed_target() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_commit_removed_target");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();

    let result = AccountSwitchCommitService::commit_with_hook(
        &selected,
        initial.active_account_id.as_deref(),
        || {
            update_accounts_atomically(|latest| {
                latest.accounts.retain(|account| account.id != selected.id);
                Ok(())
            })?;
            Ok(())
        },
    );

    assert!(result.is_err());
    let saved = load_accounts().unwrap();
    assert_eq!(saved.active_account_id, initial.active_account_id);
    assert!(saved
        .accounts
        .iter()
        .all(|account| account.id != selected.id));
    finish_fixture(env, prior_home);
}

#[test]
fn direct_switch_commit_rejects_duplicate_target_identity() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_commit_duplicate_target");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();

    let result = AccountSwitchCommitService::commit_with_hook(
        &selected,
        initial.active_account_id.as_deref(),
        || {
            update_accounts_atomically(|latest| {
                latest.accounts.push(selected.clone());
                Ok(())
            })?;
            Ok(())
        },
    );

    assert!(result.is_err());
    let persisted: crate::models::AccountsFile =
        serde_json::from_slice(&std::fs::read(crate::storage::accounts_json_path()).unwrap())
            .unwrap();
    assert_eq!(persisted.active_account_id, initial.active_account_id);
    assert_eq!(persisted.accounts.len(), 3);
    finish_fixture(env, prior_home);
}

#[test]
fn direct_switch_commit_rejects_interleaved_identity_or_eligibility_change() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    for change in ["email", "workspace", "enabled", "relogin"] {
        let prior_home = std::env::var_os("CODEX_HOME");
        let env = fixture(&format!("switch_commit_changed_{change}"));
        let initial = load_accounts().unwrap();
        let selected = initial.accounts[1].clone();

        let result = AccountSwitchCommitService::commit_with_hook(
            &selected,
            initial.active_account_id.as_deref(),
            || {
                update_accounts_atomically(|latest| {
                    let current = &mut latest.accounts[1];
                    match change {
                        "email" => current.email = "changed@example.test".into(),
                        "workspace" => current.account_id = "changed-workspace".into(),
                        "enabled" => current.enabled = false,
                        "relogin" => current.last_error = Some("401 unauthorized".into()),
                        _ => unreachable!(),
                    }
                    Ok(())
                })?;
                Ok(())
            },
        );

        assert!(result.is_err(), "{change} change must block commit");
        let saved = load_accounts().unwrap();
        assert_eq!(saved.active_account_id, initial.active_account_id);
        assert_eq!(saved.accounts[1].tokens, selected.tokens);
        finish_fixture(env, prior_home);
    }
}

#[test]
fn post_commit_auth_change_fails_closed_without_overwriting_external_auth() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_post_commit_auth_change");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();
    let mut committed = read_active_auth_json().unwrap();
    committed.tokens = Some(selected.tokens.clone());
    write_active_auth_json(&committed).unwrap();
    AccountSwitchCommitService::commit(&selected, initial.active_account_id.as_deref()).unwrap();
    let mut external = committed.clone();
    external.extra.insert(
        "desktop_extension".into(),
        serde_json::json!({"revision": 2}),
    );

    let result = AccountSwitchCommitService::verify_auth_after_commit_with(&committed, || {
        write_active_auth_json(&external)
    });

    assert!(result.is_err());
    assert_eq!(read_active_auth_json().unwrap(), external);
    assert_eq!(
        load_accounts().unwrap().active_account_id.as_deref(),
        Some(selected.id.as_str())
    );
    finish_fixture(env, prior_home);
}

#[test]
fn direct_switch_commit_rejects_interleaved_active_account_change() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_commit_changed_active_id");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();

    let result = AccountSwitchCommitService::commit_with_hook(
        &selected,
        initial.active_account_id.as_deref(),
        || {
            update_accounts_atomically(|latest| {
                let third = TestAccountSpec {
                    id: "third",
                    email: "third@example.test",
                    plan: "plus",
                    sprint_pct: 100.0,
                    ..TestAccountSpec::default()
                }
                .build();
                latest.active_account_id = Some(third.id.clone());
                latest.accounts.push(third);
                Ok(())
            })?;
            Ok(())
        },
    );

    assert!(result.is_err());
    let saved = load_accounts().unwrap();
    assert_ne!(
        saved.active_account_id.as_deref(),
        Some(selected.id.as_str())
    );
    finish_fixture(env, prior_home);
}

#[test]
fn direct_switch_commit_accepts_target_already_selected_by_another_registry_writer() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|e| e.into_inner());
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = fixture("switch_commit_target_already_active");
    let initial = load_accounts().unwrap();
    let selected = initial.accounts[1].clone();

    AccountSwitchCommitService::commit_with_hook(
        &selected,
        initial.active_account_id.as_deref(),
        || {
            update_accounts_atomically(|latest| {
                latest.active_account_id = Some(selected.id.clone());
                latest.settings.auto_switch_enabled = false;
                Ok(())
            })?;
            Ok(())
        },
    )
    .unwrap();

    let saved = load_accounts().unwrap();
    assert_eq!(
        saved.active_account_id.as_deref(),
        Some(selected.id.as_str())
    );
    assert!(!saved.settings.auto_switch_enabled);
    finish_fixture(env, prior_home);
}
