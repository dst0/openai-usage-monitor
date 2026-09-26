use super::relogin_registry_commit_service::ReloginRegistryCommitService;
use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{load_accounts, save_accounts, write_active_auth_json};

fn account(id: &str, email: &str, workspace: &str) -> AccountConfig {
    serde_json::from_value(serde_json::json!({
        "id": id,
        "email": email,
        "account_id": workspace,
        "tokens": {"access_token": "old-access", "refresh_token": "old-refresh", "account_id": workspace}
    }))
    .unwrap()
}

fn fixture(active: bool) -> (AccountConfig, AccountsFile) {
    let original = account(
        "owner@example.com:workspace-1",
        "owner@example.com",
        "workspace-1",
    );
    let mut other = account(
        "other@example.com:workspace-2",
        "other@example.com",
        "workspace-2",
    );
    other.tokens.access_token = "other-access".into();
    other.tokens.refresh_token = Some("other-refresh".into());
    let mut updated = original.clone();
    updated.tokens.access_token = "browser-access".into();
    updated.tokens.refresh_token = Some("browser-refresh".into());
    let file = AccountsFile {
        active_account_id: Some(if active {
            original.id.clone()
        } else {
            other.id.clone()
        }),
        settings: Default::default(),
        accounts: vec![updated, other],
    };
    (original, file)
}

#[test]
fn active_relogin_registry_commit_preserves_concurrent_other_account_and_settings() {
    let env = crate::distribution::test_helper::TestEnv::new("relogin_active_registry_merge");
    let (original, staged) = fixture(true);
    let mut latest = staged.clone();
    latest.accounts[0] = original.clone();
    latest.accounts[1].tokens.refresh_token = Some("other-new-refresh".into());
    latest.settings.poll_interval_seconds = 127;
    save_accounts(&latest).unwrap();
    let expected_auth = AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(staged.accounts[0].tokens.clone()),
        last_refresh: None,
        extra: Default::default(),
    };
    write_active_auth_json(&expected_auth).unwrap();

    ReloginRegistryCommitService::new(&original, &staged.accounts[0].id)
        .commit(&staged, Some(&expected_auth))
        .unwrap();
    let saved = load_accounts().unwrap();
    assert_eq!(saved.accounts[0].tokens, staged.accounts[0].tokens);
    assert_eq!(saved.accounts[1].tokens, latest.accounts[1].tokens);
    assert_eq!(saved.settings.poll_interval_seconds, 127);
    drop(env);
}

#[test]
fn inactive_relogin_registry_commit_preserves_concurrent_other_account_and_settings() {
    let env = crate::distribution::test_helper::TestEnv::new("relogin_inactive_registry_merge");
    let (original, staged) = fixture(false);
    let mut latest = staged.clone();
    latest.accounts[0] = original.clone();
    latest.accounts[1].tokens.refresh_token = Some("other-new-refresh".into());
    latest.settings.poll_interval_seconds = 127;
    save_accounts(&latest).unwrap();

    ReloginRegistryCommitService::new(&original, &staged.accounts[0].id)
        .commit(&staged, None)
        .unwrap();
    let saved = load_accounts().unwrap();
    assert_eq!(saved.accounts[0].tokens, staged.accounts[0].tokens);
    assert_eq!(saved.accounts[1].tokens, latest.accounts[1].tokens);
    assert_eq!(saved.settings.poll_interval_seconds, 127);
    drop(env);
}

#[test]
fn active_relogin_registry_commit_rechecks_live_auth_under_registry_lock() {
    let env = crate::distribution::test_helper::TestEnv::new("relogin_registry_auth_changed");
    let (original, staged) = fixture(true);
    let mut latest = staged.clone();
    latest.accounts[0] = original.clone();
    save_accounts(&latest).unwrap();
    let expected_auth = AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(staged.accounts[0].tokens.clone()),
        last_refresh: None,
        extra: Default::default(),
    };
    let mut changed_auth = expected_auth.clone();
    changed_auth.tokens.as_mut().unwrap().refresh_token = Some("desktop-rotated".into());
    write_active_auth_json(&changed_auth).unwrap();

    let error = ReloginRegistryCommitService::new(&original, &staged.accounts[0].id)
        .commit(&staged, Some(&expected_auth))
        .unwrap_err();
    assert!(error.contains("Active credentials changed"));
    assert_eq!(load_accounts().unwrap().accounts[0].tokens, original.tokens);
    drop(env);
}
