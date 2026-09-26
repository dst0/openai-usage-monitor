use super::{add_account_from_tokens_with, remove_account_with_hook, save_current_as};
use crate::distribution::test_helper::{make_account, TestEnv};
use crate::models::{AccountsFile, AuthJson};
use crate::storage::{load_accounts, save_accounts, write_active_auth_json};

#[test]
fn account_removal_keeps_concurrent_account_and_fresh_credentials() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("remove_concurrent_account");
    env.populate(
        vec![
            make_account(
                "first",
                None,
                "first@example.test",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            ),
            make_account(
                "second",
                None,
                "second@example.test",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            ),
        ],
        Some("second"),
        None,
    );

    remove_account_with_hook("first", || {
        let mut newer = load_accounts()?;
        newer.accounts.push(make_account(
            "third",
            None,
            "third@example.test",
            "team",
            100.0,
            None,
            0,
            None,
            None,
        ));
        newer.accounts[1].tokens.refresh_token = Some("fresh-refresh".into());
        newer.settings.auto_switch_enabled = false;
        save_accounts(&newer)
    })
    .unwrap();

    let saved = load_accounts().unwrap();
    assert_eq!(saved.accounts.len(), 2);
    assert!(saved
        .accounts
        .iter()
        .any(|account| account.email == "third@example.test"));
    assert!(!saved.settings.auto_switch_enabled);
    assert_eq!(
        saved
            .accounts
            .iter()
            .find(|account| account.email == "second@example.test")
            .unwrap()
            .tokens
            .refresh_token
            .as_deref(),
        Some("fresh-refresh")
    );
    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn account_addition_merges_into_newer_registry_without_replaying_stale_settings() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("add_concurrent_registry_change");
    env.populate(
        vec![make_account(
            "existing",
            None,
            "existing@example.test",
            "team",
            100.0,
            None,
            0,
            None,
            None,
        )],
        Some("existing"),
        None,
    );
    let mut stale = load_accounts().unwrap();
    let added_tokens = make_account(
        "added",
        None,
        "added@example.test",
        "team",
        100.0,
        None,
        0,
        None,
        None,
    )
    .tokens;

    add_account_from_tokens_with(
        &mut stale,
        "added",
        added_tokens,
        true,
        |_| {},
        || {
            let mut newer = load_accounts()?;
            newer.accounts.push(make_account(
                "concurrent",
                None,
                "concurrent@example.test",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            ));
            newer.accounts[0].tokens.refresh_token = Some("newer-refresh".into());
            newer.settings.auto_switch_enabled = false;
            save_accounts(&newer)
        },
    )
    .unwrap();

    let saved = load_accounts().unwrap();
    assert_eq!(saved.accounts.len(), 3);
    assert!(saved
        .accounts
        .iter()
        .any(|account| account.account_id == "concurrent"));
    assert!(saved
        .accounts
        .iter()
        .any(|account| account.account_id == "added"));
    assert!(!saved.settings.auto_switch_enabled);
    assert_eq!(
        saved.accounts[0].tokens.refresh_token.as_deref(),
        Some("newer-refresh")
    );
    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn first_account_creates_missing_registry_without_a_stale_snapshot_write() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("first_account_atomic_initialize");
    let mut snapshot = AccountsFile::default();
    let tokens = make_account(
        "first",
        None,
        "first@example.test",
        "team",
        100.0,
        None,
        0,
        None,
        None,
    )
    .tokens;
    add_account_from_tokens_with(&mut snapshot, "first", tokens, false, |_| {}, || Ok(())).unwrap();
    let saved = load_accounts().unwrap();
    assert_eq!(saved.accounts.len(), 1);
    assert_eq!(snapshot.accounts[0].id, saved.accounts[0].id);
    assert_eq!(snapshot.accounts[0].tokens, saved.accounts[0].tokens);
    drop(env);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn save_current_refuses_to_replace_an_unreadable_registry() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let env = TestEnv::new("save_current_corrupt_registry");
    let auth = AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(
            make_account(
                "current",
                None,
                "current@example.test",
                "team",
                100.0,
                None,
                0,
                None,
                None,
            )
            .tokens,
        ),
        last_refresh: None,
        extra: Default::default(),
    };
    write_active_auth_json(&auth).unwrap();
    std::fs::write(env.home().join("accounts.json"), b"invalid registry").unwrap();
    assert!(save_current_as("current").is_err());
    assert_eq!(
        std::fs::read(env.home().join("accounts.json")).unwrap(),
        b"invalid registry"
    );
    drop(env);
    std::env::remove_var("CODEX_HOME");
}
