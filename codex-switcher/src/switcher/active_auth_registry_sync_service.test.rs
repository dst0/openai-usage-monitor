use super::ActiveAuthRegistrySyncService;
use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens, Settings};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};

fn tokens(email: &str, provider: &str, generation: &str) -> AuthTokens {
    let claims = URL_SAFE_NO_PAD.encode(format!(r#"{{"email":"{email}"}}"#));
    AuthTokens {
        access_token: format!("header.{claims}.signature-{generation}"),
        refresh_token: Some(format!("refresh-{generation}")),
        id_token: None,
        account_id: Some(provider.into()),
        extra: Default::default(),
    }
}

fn account(id: &str, email: &str, provider: &str, generation: &str) -> AccountConfig {
    AccountConfig {
        id: id.into(),
        name: None,
        email: email.into(),
        plan_type: "team".into(),
        account_id: provider.into(),
        tokens: tokens(email, provider, generation),
        enabled: true,
        priority: 0,
        last_primary_percentage: 100.0,
        last_reset_time: None,
        last_reset_after_seconds: None,
        last_weekly_percentage: None,
        last_weekly_reset_time: None,
        last_weekly_reset_after_seconds: None,
        last_credits: None,
        last_error: None,
        last_checked: None,
        plan_multiplier: None,
        multiplier_is_manual: None,
        last_multiplier_checked: None,
        organization_name: None,
    }
}

fn auth(tokens: AuthTokens) -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(tokens),
        last_refresh: None,
        extra: Default::default(),
    }
}

#[test]
fn rotated_desktop_token_updates_only_the_unique_matching_saved_account() {
    let mut accounts = AccountsFile {
        active_account_id: Some("first".into()),
        settings: Settings::default(),
        accounts: vec![
            account("first", "first@example.test", "provider-1", "old"),
            account("second", "second@example.test", "provider-2", "other"),
        ],
    };
    let second_before = accounts.accounts[1].tokens.clone();
    let latest = tokens("first@example.test", "provider-1", "new");
    assert!(
        ActiveAuthRegistrySyncService::reconcile(&mut accounts, &auth(latest.clone())).unwrap()
    );
    assert_eq!(accounts.accounts[0].tokens, latest);
    assert_eq!(accounts.accounts[1].tokens, second_before);
    assert_eq!(accounts.active_account_id.as_deref(), Some("first"));
}

#[test]
fn ambiguous_or_cross_account_identity_cannot_rewrite_saved_tokens() {
    let mut accounts = AccountsFile {
        active_account_id: Some("first".into()),
        settings: Settings::default(),
        accounts: vec![
            account("first", "same@example.test", "provider-1", "old-1"),
            account("duplicate", "same@example.test", "provider-1", "old-2"),
        ],
    };
    let before = accounts
        .accounts
        .iter()
        .map(|a| a.tokens.clone())
        .collect::<Vec<_>>();
    assert!(ActiveAuthRegistrySyncService::reconcile(
        &mut accounts,
        &auth(tokens("same@example.test", "provider-1", "new")),
    )
    .is_err());
    assert_eq!(
        accounts
            .accounts
            .iter()
            .map(|a| a.tokens.clone())
            .collect::<Vec<_>>(),
        before
    );
    accounts.accounts.pop();
    assert!(ActiveAuthRegistrySyncService::reconcile(
        &mut accounts,
        &auth(tokens("other@example.test", "provider-1", "new")),
    )
    .is_err());
    assert_eq!(accounts.accounts[0].tokens, before[0]);
    let mut non_chatgpt = auth(tokens("same@example.test", "provider-1", "new"));
    non_chatgpt.auth_mode = Some("apikey".into());
    assert!(ActiveAuthRegistrySyncService::reconcile(&mut accounts, &non_chatgpt).is_err());
    assert_eq!(accounts.accounts[0].tokens, before[0]);
}

#[test]
fn conflicting_id_and_access_emails_cannot_bind_active_desktop_auth() {
    let mut accounts = AccountsFile {
        active_account_id: Some("first".into()),
        settings: Settings::default(),
        accounts: vec![account("first", "first@example.test", "provider-1", "old")],
    };
    let old = accounts.accounts[0].tokens.clone();
    let mut conflicting = tokens("another@example.test", "provider-1", "new");
    conflicting.id_token = Some(tokens("first@example.test", "provider-1", "id").access_token);
    assert!(ActiveAuthRegistrySyncService::reconcile(&mut accounts, &auth(conflicting)).is_err());
    assert_eq!(accounts.accounts[0].tokens, old);
}

#[test]
fn latest_desktop_refresh_is_persisted_before_auth_replacement() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let prior_home = std::env::var_os("CODEX_HOME");
    let home = std::env::temp_dir().join(format!("codex-active-auth-sync-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CODEX_HOME", &home);

    let mut accounts = AccountsFile {
        active_account_id: Some("first".into()),
        settings: Settings::default(),
        accounts: vec![account("first", "first@example.test", "provider-1", "old")],
    };
    crate::storage::save_accounts(&accounts).unwrap();
    let latest = tokens("first@example.test", "provider-1", "new");
    crate::storage::write_active_auth_json(&auth(latest.clone())).unwrap();

    let snapshot = ActiveAuthRegistrySyncService::sync_from_disk(&mut accounts)
        .unwrap()
        .unwrap();
    assert_eq!(snapshot.tokens.as_ref(), Some(&latest));
    assert_eq!(accounts.accounts[0].tokens, latest);
    assert_eq!(
        crate::storage::load_accounts().unwrap().accounts[0].tokens,
        latest
    );

    if let Some(prior) = prior_home {
        std::env::set_var("CODEX_HOME", prior);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn active_auth_sync_preserves_registry_change_after_initial_snapshot() {
    let _lock = crate::setup::TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let prior_home = std::env::var_os("CODEX_HOME");
    let home = std::env::temp_dir().join(format!("codex-active-sync-race-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CODEX_HOME", &home);
    let mut snapshot = AccountsFile {
        active_account_id: Some("first".into()),
        settings: Settings::default(),
        accounts: vec![account("first", "first@example.test", "provider-1", "old")],
    };
    crate::storage::save_accounts(&snapshot).unwrap();
    crate::storage::write_active_auth_json(&auth(tokens(
        "first@example.test",
        "provider-1",
        "new",
    )))
    .unwrap();
    let result = ActiveAuthRegistrySyncService::sync_from_disk_with_hook(&mut snapshot, || {
        let mut latest = crate::storage::load_accounts()?;
        latest.settings.poll_interval_seconds = 127;
        crate::storage::save_accounts(&latest)
    });
    let saved = crate::storage::load_accounts().unwrap();
    if let Some(prior) = prior_home {
        std::env::set_var("CODEX_HOME", prior);
    } else {
        std::env::remove_var("CODEX_HOME");
    }
    std::fs::remove_dir_all(home).unwrap();
    result.unwrap();
    assert_eq!(saved.settings.poll_interval_seconds, 127);
    assert_eq!(saved.accounts[0].tokens, snapshot.accounts[0].tokens);
}
