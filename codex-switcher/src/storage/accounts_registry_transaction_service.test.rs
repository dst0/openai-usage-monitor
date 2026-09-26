use super::AccountsRegistryTransactionService;
use crate::models::{AccountsFile, AuthJson};
use crate::storage::write_active_auth_json;
use std::os::unix::fs::{symlink, PermissionsExt};

#[test]
fn registry_write_does_not_follow_a_preexisting_temporary_symlink() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("registry_temp_symlink");
    let victim = env.home().join("unrelated.txt");
    std::fs::write(&victim, "leave intact").unwrap();
    let old_temp = env
        .home()
        .join(format!("accounts.{}.tmp.json", std::process::id()));
    symlink(&victim, &old_temp).unwrap();

    let outcome = AccountsRegistryTransactionService::save(&AccountsFile::default());
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "leave intact");
    assert!(outcome.is_ok());
    assert_eq!(
        std::fs::metadata(env.home().join("accounts.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn active_auth_write_does_not_follow_a_preexisting_temporary_symlink() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("auth_temp_symlink");
    let victim = env.home().join("unrelated.txt");
    std::fs::write(&victim, "leave intact").unwrap();
    let old_temp = env
        .home()
        .join(format!("auth.{}.tmp.json", std::process::id()));
    symlink(&victim, &old_temp).unwrap();
    let auth: AuthJson = serde_json::from_value(serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": { "access_token": "opaque-test-value" }
    }))
    .unwrap();

    let outcome = write_active_auth_json(&auth);
    assert_eq!(std::fs::read_to_string(&victim).unwrap(), "leave intact");
    assert!(outcome.is_ok());
    assert_eq!(
        std::fs::metadata(env.home().join("auth.json"))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );

    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn registry_autoheal_cannot_replay_stale_tokens_after_a_concurrent_commit() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("registry_autoheal_race");
    let original = crate::distribution::test_account_spec::TestAccountSpec {
        id: "owner",
        email: "owner@example.test",
        plan: "team",
        sprint_pct: 50.0,
        ..crate::distribution::test_account_spec::TestAccountSpec::default()
    }
    .build();
    let duplicated = AccountsFile {
        active_account_id: Some(original.id.clone()),
        settings: Default::default(),
        accounts: vec![original.clone(), original.clone()],
    };
    AccountsRegistryTransactionService::save(&duplicated).unwrap();
    let mut newer = duplicated;
    newer.accounts.truncate(1);
    newer.accounts[0].tokens.refresh_token = Some("fresh-test-value".into());

    let loaded = crate::storage::load_accounts_with_hooks(
        || {},
        || AccountsRegistryTransactionService::save(&newer).unwrap(),
    )
    .unwrap();
    assert_eq!(
        loaded.accounts[0].tokens.refresh_token.as_deref(),
        Some("fresh-test-value")
    );
    assert_eq!(
        crate::storage::load_accounts().unwrap().accounts[0]
            .tokens
            .refresh_token
            .as_deref(),
        Some("fresh-test-value")
    );

    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn registry_autoimport_cannot_replace_a_concurrent_registry_creation() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("registry_autoimport_race");
    let auth: AuthJson = serde_json::from_value(serde_json::json!({
        "auth_mode": "chatgpt",
        "tokens": { "access_token": "opaque-test-value", "account_id": "workspace-1" }
    }))
    .unwrap();
    write_active_auth_json(&auth).unwrap();
    let newer_account = crate::distribution::test_account_spec::TestAccountSpec {
        id: "newer",
        email: "owner@example.test",
        plan: "team",
        sprint_pct: 50.0,
        ..crate::distribution::test_account_spec::TestAccountSpec::default()
    }
    .build();
    let mut newer = AccountsFile {
        active_account_id: Some(newer_account.id.clone()),
        settings: Default::default(),
        accounts: vec![newer_account],
    };
    newer.settings.poll_interval_seconds = 127;

    let loaded = crate::storage::load_accounts_with_hooks(
        || AccountsRegistryTransactionService::save(&newer).unwrap(),
        || {},
    )
    .unwrap();
    assert_eq!(loaded.active_account_id, newer.active_account_id);
    assert_eq!(loaded.settings.poll_interval_seconds, 127);
    assert_eq!(crate::storage::load_accounts().unwrap().accounts.len(), 1);

    drop(env);
    std::env::remove_var("CODEX_HOME");
}
