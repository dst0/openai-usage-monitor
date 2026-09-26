use super::resolve_account_with_sync;
use crate::distribution::test_helper::TestEnv;
use crate::models::{AccountConfig, AccountsFile, AuthJson, Settings};
use crate::storage::{
    load_accounts, read_active_auth_json, save_accounts, update_accounts_atomically,
    write_active_auth_json,
};
use crate::switcher::ActiveAuthRegistrySyncService;

fn account(id: &str, name: &str) -> AccountConfig {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "name": name,
        "email": format!("{name}@example.test"),
        "account_id": format!("workspace-{name}"),
        "tokens": {
            "access_token": format!("fixture-{name}"),
            "account_id": format!("workspace-{name}")
        }
    }))
    .unwrap()
}

fn registry() -> AccountsFile {
    AccountsFile {
        active_account_id: Some("other@example.test:workspace-other".into()),
        settings: Settings::default(),
        accounts: vec![
            account("target@example.test:workspace-target", "target"),
            account("other@example.test:workspace-other", "other"),
        ],
    }
}

fn active_auth(accounts: &AccountsFile) -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(accounts.accounts[1].tokens.clone()),
        last_refresh: None,
        extra: Default::default(),
    }
}

#[test]
fn selected_account_survives_registry_reorder_during_auth_sync() {
    let mut accounts = registry();
    let (selected, auth) = resolve_account_with_sync(&mut accounts, "target", |fresh| {
        fresh.accounts.swap(0, 1);
        Ok(None)
    })
    .unwrap();

    assert_eq!(selected.id, "target@example.test:workspace-target");
    assert_eq!(selected.tokens.access_token, "fixture-target");
    assert!(auth.is_none());
    assert_eq!(
        accounts.accounts[0].id,
        "other@example.test:workspace-other"
    );
}

#[test]
fn removed_or_duplicated_target_fails_before_credential_selection() {
    let mut removed = registry();
    assert!(resolve_account_with_sync(&mut removed, "target", |fresh| {
        fresh.accounts.remove(0);
        Ok(None)
    })
    .is_err());

    let mut duplicated = registry();
    assert!(
        resolve_account_with_sync(&mut duplicated, "target", |fresh| {
            fresh.accounts.push(fresh.accounts[0].clone());
            Ok(None)
        })
        .is_err()
    );
}

#[test]
fn disk_registry_reorder_before_real_auth_sync_uses_fresh_target_tokens() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = TestEnv::new("target_reorder_before_auth_sync");
    let initial = registry();
    let auth = active_auth(&initial);
    save_accounts(&initial).unwrap();
    write_active_auth_json(&auth).unwrap();
    let mut snapshot = load_accounts().unwrap();

    let (selected, observed) = resolve_account_with_sync(&mut snapshot, "target", |accounts| {
        update_accounts_atomically(|fresh| {
            fresh.accounts.swap(0, 1);
            fresh.accounts[1].tokens.access_token = "fixture-target-fresh".into();
            Ok(())
        })?;
        ActiveAuthRegistrySyncService::sync_from_disk(accounts)
    })
    .unwrap();

    assert_eq!(selected.id, "target@example.test:workspace-target");
    assert_eq!(selected.tokens.access_token, "fixture-target-fresh");
    assert_eq!(
        snapshot.accounts[0].id,
        "other@example.test:workspace-other"
    );
    assert_eq!(load_accounts().unwrap().accounts[1].tokens, selected.tokens);
    assert_eq!(observed, Some(auth.clone()));
    assert_eq!(read_active_auth_json().unwrap(), auth);

    drop(env);
    if let Some(prior) = prior_home {
        std::env::set_var("CODEX_HOME", prior);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
}

#[test]
fn disk_registry_target_removal_before_real_auth_sync_preserves_active_auth() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let prior_home = std::env::var_os("CODEX_HOME");
    let env = TestEnv::new("target_removed_before_auth_sync");
    let initial = registry();
    let auth = active_auth(&initial);
    save_accounts(&initial).unwrap();
    write_active_auth_json(&auth).unwrap();
    let mut snapshot = load_accounts().unwrap();

    let error = resolve_account_with_sync(&mut snapshot, "target", |accounts| {
        update_accounts_atomically(|fresh| {
            fresh.accounts.remove(0);
            Ok(())
        })?;
        ActiveAuthRegistrySyncService::sync_from_disk(accounts)
    })
    .unwrap_err();

    assert!(error.contains("disappeared"), "{error}");
    assert_eq!(load_accounts().unwrap().accounts.len(), 1);
    assert_eq!(read_active_auth_json().unwrap(), auth);

    drop(env);
    if let Some(prior) = prior_home {
        std::env::set_var("CODEX_HOME", prior);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
}
