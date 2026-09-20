use super::DaemonAccountSyncService;
use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens, Settings};

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
        }),
        last_refresh: None,
    }
}

#[test]
fn syncs_matching_workspace_tokens() {
    let mut file = AccountsFile {
        active_account_id: None,
        settings: Settings::default(),
        accounts: vec![account("main", "user@example.com", "workspace", "old")],
    };

    assert!(DaemonAccountSyncService::sync_active_tokens_from_auth_obj(
        &mut file,
        &auth("fresh", Some("workspace"), None),
    ));
    assert_eq!(file.active_account_id.as_deref(), Some("main"));
    assert_eq!(file.accounts[0].tokens.access_token, "fresh");
    assert!(file.accounts[0].last_error.is_none());
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
