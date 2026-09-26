use super::DaemonAccountSyncService;
use crate::distribution::test_helper::TestEnv;
use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens, Settings};
use crate::storage::{load_accounts, save_accounts, write_active_auth_json};
use base64::Engine;

fn account(id: &str, email: &str, workspace: &str, token: &str) -> AccountConfig {
    AccountConfig {
        id: id.to_string(),
        name: None,
        email: email.to_string(),
        plan_type: "team".to_string(),
        account_id: workspace.to_string(),
        tokens: AuthTokens {
            access_token: token.to_string(),
            refresh_token: Some("refresh".to_string()),
            id_token: None,
            account_id: Some(workspace.to_string()),
            extra: Default::default(),
        },
        enabled: true,
        priority: 0,
        last_primary_percentage: 100.0,
        last_reset_time: None,
        last_reset_after_seconds: None,
        last_weekly_percentage: None,
        last_weekly_reset_time: None,
        last_weekly_reset_after_seconds: None,
        last_credits: None,
        last_error: Some("HTTP 401 Unauthorized".to_string()),
        last_checked: None,
        plan_multiplier: None,
        multiplier_is_manual: None,
        last_multiplier_checked: None,
        organization_name: None,
    }
}

fn auth(token: &str, workspace: Option<&str>, id_token: Option<String>) -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".to_string()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: token.to_string(),
            refresh_token: Some("new-refresh".to_string()),
            id_token,
            account_id: workspace.map(ToString::to_string),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    }
}

fn identity_token(email: &str) -> String {
    let payload = serde_json::json!({ "email": email });
    format!(
        "header.{}.signature",
        base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(payload.to_string())
    )
}

#[test]
fn syncs_matching_workspace_tokens() {
    let mut file = AccountsFile {
        active_account_id: None,
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "old")],
    };
    file.accounts[0].tokens.refresh_token = Some("new-refresh".into());

    assert!(DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("fresh", Some("workspace"), None),
    ));
    assert_eq!(file.active_account_id.as_deref(), Some("main"));
    assert_eq!(file.accounts[0].tokens.access_token, "fresh");
    assert!(file.accounts[0].last_error.is_none());
}

#[test]
fn rotated_tokens_without_email_cannot_bind_by_workspace_alone() {
    let mut file = AccountsFile {
        active_account_id: Some("main".into()),
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "old")],
    };

    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("unknown-session", Some("workspace"), None),
    ));
    assert_eq!(file.accounts[0].tokens.access_token, "old");
    assert_eq!(
        file.accounts[0].tokens.refresh_token.as_deref(),
        Some("refresh")
    );
}

#[test]
fn rotated_tokens_require_both_email_and_known_workspace_identity() {
    let mut file = AccountsFile {
        active_account_id: Some("main".into()),
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "old")],
    };
    let verified_email = Some(identity_token("user@example.com"));
    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("rotated", None, verified_email.clone()),
    ));
    assert_eq!(file.accounts[0].tokens.access_token, "old");
    assert!(DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("rotated", Some("workspace"), verified_email),
    ));
    assert_eq!(file.accounts[0].tokens.access_token, "rotated");
}

#[test]
fn conflicting_id_and_access_emails_cannot_sync_desktop_tokens() {
    let mut file = AccountsFile {
        active_account_id: Some("main".into()),
        settings: Settings::default(),
        accounts: vec![account("main", "owner@example.com", "workspace", "old")],
    };
    let original = file.accounts[0].tokens.clone();
    let mut conflicting = auth(
        "opaque",
        Some("workspace"),
        Some(identity_token("owner@example.com")),
    );
    conflicting.tokens.as_mut().unwrap().access_token = identity_token("another@example.com");
    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &conflicting,
    ));
    assert_eq!(file.accounts[0].tokens, original);
}

#[test]
fn unchanged_and_empty_tokens_are_ignored() {
    let mut file = AccountsFile {
        active_account_id: Some("main".to_string()),
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "same")],
    };
    file.accounts[0].tokens.refresh_token = Some("new-refresh".to_string());
    file.accounts[0].last_error = None;

    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("same", Some("workspace"), None),
    ));
    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("   ", Some("workspace"), None),
    ));
}

#[test]
fn unknown_tokens_without_real_email_are_not_registered() {
    let mut file = AccountsFile {
        active_account_id: Some("main".to_string()),
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "valid")],
    };

    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("unknown", Some("other-workspace"), None),
    ));
    assert_eq!(file.accounts.len(), 1);
}

#[test]
fn organization_names_cross_pollinate_only_real_workspaces() {
    let mut accounts = vec![
        account("one", "one@example.com", "shared", "one"),
        account("two", "two@example.com", "shared", "two"),
        account("three", "three@example.com", "default", "three"),
    ];
    accounts[1].organization_name = Some("Organization".to_string());

    DaemonAccountSyncService::cross_pollinate_organization_names(&mut accounts);

    assert_eq!(
        accounts[0].organization_name.as_deref(),
        Some("Organization")
    );
    assert_eq!(
        accounts[1].organization_name.as_deref(),
        Some("Organization")
    );
    assert!(accounts[2].organization_name.is_none());
}

#[test]
fn conflicting_identity_and_saved_refresh_token_fail_closed() {
    let mut first = account(
        "first",
        "first@example.com",
        "shared-workspace",
        "first-old",
    );
    first.tokens.refresh_token = Some("first-old-refresh".into());
    let mut second = account(
        "second",
        "second@example.com",
        "shared-workspace",
        "second-old",
    );
    second.tokens.refresh_token = Some("new-refresh".into());
    let mut file = AccountsFile {
        active_account_id: Some("second".into()),
        settings: Settings::default(),
        accounts: vec![first, second],
    };

    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth(
            "first-fresh",
            Some("shared-workspace"),
            Some(identity_token("first@example.com"))
        ),
    ));

    assert_eq!(file.active_account_id.as_deref(), Some("second"));
    assert_eq!(file.accounts[0].tokens.access_token, "first-old");
    assert_eq!(file.accounts[1].tokens.access_token, "second-old");
}

#[test]
fn ambiguous_desktop_token_without_identity_does_not_bind_arbitrarily() {
    let mut first = account(
        "first",
        "first@example.com",
        "shared-workspace",
        "first-old",
    );
    first.tokens.refresh_token = Some("new-refresh".into());
    let mut second = account(
        "second",
        "second@example.com",
        "shared-workspace",
        "second-old",
    );
    second.tokens.refresh_token = Some("new-refresh".into());
    let mut file = AccountsFile {
        active_account_id: Some("first".into()),
        settings: Settings::default(),
        accounts: vec![first, second],
    };

    assert!(!DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("fresh", Some("shared-workspace"), None),
    ));
    assert_eq!(file.accounts[0].tokens.access_token, "first-old");
    assert_eq!(file.accounts[1].tokens.access_token, "second-old");
}

#[test]
fn auto_actions_require_exact_active_desktop_tokens() {
    let mut file = AccountsFile {
        active_account_id: Some("main".into()),
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "current")],
    };
    file.accounts[0].tokens.refresh_token = Some("new-refresh".into());
    assert!(DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &auth("current", Some("workspace"), None),
    ));
    assert!(!DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &auth("desktop-rotated", Some("workspace"), None),
    ));
    assert!(!DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &auth(
            "current",
            Some("workspace"),
            Some(identity_token("other@example.com"))
        ),
    ));
    let mut api_key_mode = auth("current", Some("workspace"), None);
    api_key_mode.auth_mode = Some("apikey".into());
    assert!(!DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &api_key_mode,
    ));
    let duplicate = file.accounts[0].clone();
    file.accounts.push(duplicate);
    assert!(!DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &auth("current", Some("workspace"), None),
    ));
    file.accounts.pop();
    let mut logged_out = auth("", None, None);
    logged_out.tokens = None;
    assert!(!DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &logged_out,
    ));
    file.active_account_id = None;
    assert!(!DaemonAccountSyncService::active_auth_matches_registry(
        &file,
        &auth("current", Some("workspace"), None),
    ));
}

#[test]
fn ambiguous_live_auth_cannot_be_persisted_to_account_registry() {
    let env = TestEnv::new("ambiguous_desktop_auth");
    let first = account(
        "first",
        "first@example.com",
        "shared-workspace",
        "first-old",
    );
    let mut second = account(
        "second",
        "second@example.com",
        "shared-workspace",
        "second-old",
    );
    second.tokens.refresh_token = Some("new-refresh".into());
    env.populate(vec![first, second], Some("second"), None);
    write_active_auth_json(&auth(
        "first-fresh",
        Some("shared-workspace"),
        Some(identity_token("first@example.com")),
    ))
    .unwrap();
    let mut registry = load_accounts().unwrap();

    assert!(DaemonAccountSyncService::sync_active_tokens(&mut registry).is_err());
    let persisted = load_accounts().unwrap();
    let active = persisted
        .accounts
        .iter()
        .find(|account| Some(account.id.as_str()) == persisted.active_account_id.as_deref())
        .unwrap();
    assert_eq!(active.email, "second@example.com");
    assert_eq!(persisted.accounts.len(), 2);
    assert_eq!(persisted.accounts[0].tokens.access_token, "first-old");
    assert_eq!(persisted.accounts[1].tokens.access_token, "second-old");
}

#[test]
fn active_sync_cannot_replay_snapshot_after_relogin_commit() {
    let env = TestEnv::new("active_sync_relogin_interleave");
    env.populate(
        vec![account(
            "main",
            "owner@example.com",
            "workspace",
            "old-access",
        )],
        Some("main"),
        None,
    );
    let older_live = auth(
        "old-access",
        Some("workspace"),
        Some(identity_token("owner@example.com")),
    );
    write_active_auth_json(&older_live).unwrap();
    let mut snapshot = load_accounts().unwrap();
    let newer_live = auth(
        "new-browser-access",
        Some("workspace"),
        Some(identity_token("owner@example.com")),
    );
    DaemonAccountSyncService::sync_active_tokens_with_hook(&mut snapshot, || {
        write_active_auth_json(&newer_live)?;
        let mut latest = load_accounts()?;
        latest.accounts[0].tokens = newer_live.tokens.clone().unwrap();
        save_accounts(&latest)
    })
    .unwrap();
    let persisted = load_accounts().unwrap();
    assert_eq!(persisted.accounts[0].tokens, newer_live.tokens.unwrap());
    assert_eq!(snapshot.accounts[0].tokens, persisted.accounts[0].tokens);
}
