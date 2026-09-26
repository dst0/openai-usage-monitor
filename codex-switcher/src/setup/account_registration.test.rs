use super::{add_account_from_tokens_with, remove_account_with_hook, save_current_as};
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::models::{AccountsFile, AuthJson};
use crate::storage::{load_accounts, save_accounts, write_active_auth_json};

#[test]
fn account_removal_keeps_concurrent_account_and_fresh_credentials() {
    let env = TestEnv::new("remove_concurrent_account");
    env.populate(
        vec![
            TestAccountSpec {
                id: "first",
                email: "first@example.test",
                plan: "team",
                sprint_pct: 100.0,
                ..TestAccountSpec::default()
            }
            .build(),
            TestAccountSpec {
                id: "second",
                email: "second@example.test",
                plan: "team",
                sprint_pct: 100.0,
                ..TestAccountSpec::default()
            }
            .build(),
        ],
        Some("second"),
        None,
    );

    remove_account_with_hook("first", || {
        let mut newer = load_accounts()?;
        newer.accounts.push(
            TestAccountSpec {
                id: "third",
                email: "third@example.test",
                plan: "team",
                sprint_pct: 100.0,
                ..TestAccountSpec::default()
            }
            .build(),
        );
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
}

#[test]
fn account_addition_merges_into_newer_registry_without_replaying_stale_settings() {
    let env = TestEnv::new("add_concurrent_registry_change");
    env.populate(
        vec![TestAccountSpec {
            id: "existing",
            email: "existing@example.test",
            plan: "team",
            sprint_pct: 100.0,
            ..TestAccountSpec::default()
        }
        .build()],
        Some("existing"),
        None,
    );
    let mut stale = load_accounts().unwrap();
    let added_tokens = TestAccountSpec {
        id: "added",
        email: "added@example.test",
        plan: "team",
        sprint_pct: 100.0,
        ..TestAccountSpec::default()
    }
    .build()
    .tokens;

    add_account_from_tokens_with(
        &mut stale,
        "added",
        added_tokens,
        true,
        |_| {},
        || {
            let mut newer = load_accounts()?;
            newer.accounts.push(
                TestAccountSpec {
                    id: "concurrent",
                    email: "concurrent@example.test",
                    plan: "team",
                    sprint_pct: 100.0,
                    ..TestAccountSpec::default()
                }
                .build(),
            );
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
}

#[test]
fn first_account_creates_missing_registry_without_a_stale_snapshot_write() {
    let env = TestEnv::new("first_account_atomic_initialize");
    let mut snapshot = AccountsFile::default();
    let tokens = TestAccountSpec {
        id: "first",
        email: "first@example.test",
        plan: "team",
        sprint_pct: 100.0,
        ..TestAccountSpec::default()
    }
    .build()
    .tokens;
    add_account_from_tokens_with(&mut snapshot, "first", tokens, false, |_| {}, || Ok(())).unwrap();
    let saved = load_accounts().unwrap();
    assert_eq!(saved.accounts.len(), 1);
    assert_eq!(snapshot.accounts[0].id, saved.accounts[0].id);
    assert_eq!(snapshot.accounts[0].tokens, saved.accounts[0].tokens);
    drop(env);
}

#[test]
fn save_current_refuses_to_replace_an_unreadable_registry() {
    let env = TestEnv::new("save_current_corrupt_registry");
    let auth = AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(
            TestAccountSpec {
                id: "current",
                email: "current@example.test",
                plan: "team",
                sprint_pct: 100.0,
                ..TestAccountSpec::default()
            }
            .build()
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
}
