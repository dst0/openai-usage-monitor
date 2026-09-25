use super::*;
use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens};

#[test]
fn manual_credit_reset_refuses_to_race_desktop_recovery() {
    let _serial = TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let home =
        std::env::temp_dir().join(format!("codex-reset-recovery-lock-{}", std::process::id()));
    std::fs::create_dir_all(&home).unwrap();
    std::env::set_var("CODEX_HOME", &home);
    let recovery = crate::recovery::operation_lock().unwrap();
    let result = reset_account("active");
    assert!(result
        .unwrap_err()
        .contains("Another desktop switch/recovery"));
    drop(recovery);
    std::env::remove_var("CODEX_HOME");
    std::fs::remove_dir_all(home).unwrap();
}

fn make_test_account(
    id: &str,
    email: &str,
    acc_id: &str,
    refresh_token: Option<&str>,
    access_token: &str,
) -> AccountConfig {
    AccountConfig {
        id: id.to_string(),
        name: None,
        email: email.to_string(),
        plan_type: "team".to_string(),
        account_id: acc_id.to_string(),
        tokens: AuthTokens {
            access_token: access_token.to_string(),
            refresh_token: refresh_token.map(String::from),
            id_token: None,
            account_id: Some(acc_id.to_string()),
        },
        enabled: true,
        priority: 1,
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

#[test]
fn test_build_predictable_account_id() {
    assert_eq!(
        build_predictable_account_id("Dev.User@example.com ", " 3f533057-4bac-44ea "),
        "dev.user@example.com:3f533057-4bac-44ea"
    );
    assert_eq!(
        build_predictable_account_id("foo@bar.com", "default"),
        "foo@bar.com:default"
    );
    assert_eq!(
        build_predictable_account_id("foo@bar.com", ""),
        "foo@bar.com:default"
    );
}

#[test]
fn test_find_existing_account_idx_multi_vector() {
    let accounts = vec![
        make_test_account(
            "main",
            "dev.user@example.com",
            "uuid-1",
            Some("rt_1"),
            "at_1",
        ),
        make_test_account("work", "work@company.com", "uuid-2", Some("rt_2"), "at_2"),
    ];

    // 1. Match by refresh token
    assert_eq!(
        find_existing_account_idx_from_parts(
            &accounts,
            "other",
            "other@foo.com",
            "uuid-x",
            None,
            Some("rt_1"),
            None
        ),
        Some(0)
    );

    // 2. Match by access token
    assert_eq!(
        find_existing_account_idx_from_parts(
            &accounts,
            "other",
            "other@foo.com",
            "uuid-x",
            None,
            None,
            Some("at_2")
        ),
        Some(1)
    );

    // 3. Match by account_id UUID when email is not conflicting
    assert_eq!(
        find_existing_account_idx_from_parts(&accounts, "", "", "uuid-1", None, None, None),
        Some(0)
    );

    // 4. Match by email case-insensitively when workspace is unassigned
    assert_eq!(
        find_existing_account_idx_from_parts(
            &accounts,
            "",
            "  DEV.USER@EXAMPLE.COM  ",
            "",
            None,
            None,
            None
        ),
        Some(0)
    );

    // 5. Match by ID alias when email is not conflicting
    assert_eq!(
        find_existing_account_idx_from_parts(&accounts, "work", "", "uuid-x", None, None, None),
        Some(1)
    );

    // 6. Same account_id but conflicting email must NOT match
    assert_eq!(
        find_existing_account_idx_from_parts(
            &accounts,
            "other",
            "other@foo.com",
            "uuid-1",
            None,
            None,
            None
        ),
        None
    );

    // 7. Distinct account
    assert_eq!(
        find_existing_account_idx_from_parts(
            &accounts,
            "new_id",
            "new@foo.com",
            "uuid-3",
            None,
            Some("rt_3"),
            Some("at_3")
        ),
        None
    );
}

#[test]
fn test_same_email_different_workspaces_or_plans_never_merge() {
    let accounts = vec![make_test_account(
        "business",
        "dev.user@example.com",
        "uuid-team",
        Some("rt_team"),
        "at_team",
    )];

    // 1. Adding personal account with same email, but different label and different account_id
    let res1 = find_existing_account_idx_from_parts(
        &accounts,
        "dev.user@example.com-[personal]",
        "dev.user@example.com",
        "uuid-personal",
        Some("pro"),
        Some("rt_pro"),
        Some("at_pro"),
    );
    assert_eq!(res1, None);

    // 2. Deduplication check (empty id): different account_id (workspace UUID) prevents merge
    let res2 = find_existing_account_idx_from_parts(
        &accounts,
        "",
        "dev.user@example.com",
        "uuid-personal",
        Some("pro"),
        Some("rt_pro"),
        Some("at_pro"),
    );
    assert_eq!(res2, None);

    // 3. Deduplication check (empty id): different plan prevents merge
    let res3 = find_existing_account_idx_from_parts(
        &accounts,
        "",
        "dev.user@example.com",
        "default",
        Some("pro"),
        Some("rt_pro"),
        Some("at_pro"),
    );
    assert_eq!(res3, None);
}

#[test]
fn test_same_team_workspace_different_emails_never_merge() {
    let mut accounts = vec![
        make_test_account(
            "dev-alt",
            "dev.alt@example.com",
            "3f533057",
            Some("rt_1"),
            "at_1",
        ),
        make_test_account(
            "dev-primary",
            "dev.user@example.com",
            "3f533057",
            Some("rt_2"),
            "at_2",
        ),
    ];

    let _ = deduplicate_accounts(&mut accounts);
    assert_eq!(accounts.len(), 2);
    assert_eq!(accounts[0].email, "dev.alt@example.com");
    assert_eq!(accounts[0].id, "dev.alt@example.com:3f533057");
    assert_eq!(accounts[1].email, "dev.user@example.com");
    assert_eq!(accounts[1].id, "dev.user@example.com:3f533057");
}

#[test]
fn test_deduplicate_accounts_merges_identical_user() {
    let mut accounts = vec![
        make_test_account(
            "main",
            "dev.user@example.com",
            "3f533057",
            Some("rt_same"),
            "at_old",
        ),
        make_test_account(
            "dev-primary",
            "dev.user@example.com",
            "3f533057",
            Some("rt_same"),
            "at_new",
        ),
    ];

    let merged_map = deduplicate_accounts(&mut accounts);
    assert_eq!(accounts.len(), 1);
    assert_eq!(accounts[0].id, "dev.user@example.com:3f533057");
    assert_eq!(accounts[0].name.as_deref(), Some("dev-primary"));
    assert_eq!(accounts[0].tokens.access_token, "at_new");
    assert_eq!(
        merged_map.get("main").map(String::as_str),
        Some("dev.user@example.com:3f533057")
    );
}

fn make_test_jwt(email: &str) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let payload = format!(r#"{{"email":"{}"}}"#, email);
    let b64 = URL_SAFE_NO_PAD.encode(payload.as_bytes());
    format!("eyJhbGciOiJub25lIn0.{}.sig", b64)
}

#[test]
fn test_same_email_and_workspace_different_nicknames_always_merge() {
    let mut file = AccountsFile {
        active_account_id: Some("dev-account".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "dev-account",
            "dev@enterprise.example.com",
            "uuid-team",
            Some("rt_1"),
            "at_1",
        )],
    };

    // User tries saving current session under new nickname "dev-account-3"
    let tokens = AuthTokens {
        access_token: "at_updated".to_string(),
        refresh_token: Some("rt_updated".to_string()),
        id_token: Some(make_test_jwt("dev@enterprise.example.com")),
        account_id: Some("uuid-team".to_string()),
    };

    let target_id = add_account_to_accounts_file(&mut file, "dev-account-3", tokens, false);
    // Must merge with existing account, NOT create a second one!
    assert_eq!(file.accounts.len(), 1);
    assert_eq!(target_id, "dev@enterprise.example.com:uuid-team");
    assert_eq!(file.accounts[0].name.as_deref(), Some("dev-account-3"));
}

#[test]
fn test_deduplicate_accounts_file_remaps_active_id() {
    let mut file = AccountsFile {
        active_account_id: Some("main".to_string()),
        settings: Default::default(),
        accounts: vec![
            make_test_account(
                "main",
                "dev.user@example.com",
                "3f533057",
                Some("rt_same"),
                "at_old",
            ),
            make_test_account(
                "dev-primary",
                "dev.user@example.com",
                "3f533057",
                Some("rt_same"),
                "at_new",
            ),
        ],
    };

    let changed = deduplicate_accounts_file(&mut file);
    assert!(changed);
    assert_eq!(file.accounts.len(), 1);
    assert_eq!(
        file.active_account_id.as_deref(),
        Some("dev.user@example.com:3f533057")
    );
}

#[test]
fn test_add_account_preserves_active_account() {
    let mut file = AccountsFile {
        active_account_id: Some("primary@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "primary@example.com:uuid-1",
            "primary@example.com",
            "uuid-1",
            Some("rt_1"),
            "at_1",
        )],
    };

    let new_tokens = AuthTokens {
        access_token: "at_2".to_string(),
        refresh_token: Some("rt_2".to_string()),
        id_token: None,
        account_id: Some("uuid-2".to_string()),
    };

    let added_id = add_account_to_accounts_file(&mut file, "secondary", new_tokens, true);
    assert_eq!(added_id, "user@openai.com:uuid-2");
    assert_eq!(file.accounts.len(), 2);
    assert_eq!(file.accounts[1].name.as_deref(), Some("secondary"));
    // CRITICAL INVARIANT: active account MUST NOT be replaced!
    assert_eq!(
        file.active_account_id.as_deref(),
        Some("primary@example.com:uuid-1")
    );
}

#[test]
fn test_add_account_sets_active_when_none_existed() {
    let mut file = AccountsFile {
        active_account_id: None,
        settings: Default::default(),
        accounts: vec![],
    };

    let new_tokens = AuthTokens {
        access_token: "at_1".to_string(),
        refresh_token: Some("rt_1".to_string()),
        id_token: None,
        account_id: Some("uuid-1".to_string()),
    };

    let added_id = add_account_to_accounts_file(&mut file, "first", new_tokens, true);
    assert_eq!(added_id, "user@openai.com:uuid-1");
    assert_eq!(file.accounts.len(), 1);
    assert_eq!(
        file.active_account_id.as_deref(),
        Some("user@openai.com:uuid-1")
    );
}

#[test]
fn test_save_current_as_replaces_active_account() {
    let mut file = AccountsFile {
        active_account_id: Some("old@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "old@example.com:uuid-1",
            "old@example.com",
            "uuid-1",
            Some("rt_1"),
            "at_1",
        )],
    };

    let current_tokens = AuthTokens {
        access_token: "at_2".to_string(),
        refresh_token: Some("rt_2".to_string()),
        id_token: None,
        account_id: Some("uuid-2".to_string()),
    };

    let saved_id = add_account_to_accounts_file(&mut file, "new_active", current_tokens, false);
    assert_eq!(saved_id, "user@openai.com:uuid-2");
    assert_eq!(file.accounts.len(), 2);
    assert_eq!(
        file.active_account_id.as_deref(),
        Some("user@openai.com:uuid-2")
    );
}

#[test]
fn test_rename_account_updates_nickname() {
    let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir = std::env::temp_dir().join(format!("codex_rename_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    std::env::set_var("CODEX_HOME", &temp_dir);

    let mut file = AccountsFile {
        active_account_id: Some("user1@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![
            make_test_account(
                "user1@example.com:uuid-1",
                "user1@example.com",
                "uuid-1",
                Some("rt_1"),
                "at_1",
            ),
            make_test_account(
                "user2@example.com:uuid-2",
                "user2@example.com",
                "uuid-2",
                Some("rt_2"),
                "at_2",
            ),
        ],
    };
    file.accounts[0].name = Some("first".to_string());
    file.accounts[1].name = Some("second".to_string());
    crate::storage::save_accounts(&file).unwrap();

    // 1. Rename first account to "work"
    assert!(rename_account("first", Some("work")).is_ok());
    let loaded = crate::storage::load_accounts().unwrap();
    assert_eq!(loaded.accounts[0].name.as_deref(), Some("work"));

    // 2. Renaming to existing nickname "second" must fail with error (avoids duplicate labels!)
    assert!(rename_account("work", Some("second")).is_err());

    // 3. Clear nickname
    assert!(rename_account("work", None).is_ok());
    let loaded = crate::storage::load_accounts().unwrap();
    assert_eq!(loaded.accounts[0].name, None);

    let _ = std::fs::remove_dir_all(&temp_dir);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn test_apply_relogin_successful_update() {
    let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir =
        std::env::temp_dir().join(format!("codex_relogin_succ_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    std::env::set_var("CODEX_HOME", &temp_dir);

    let mut file = AccountsFile {
        active_account_id: Some("test-user@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![{
            let mut acc = make_test_account(
                "test-user@example.com:uuid-1",
                "test-user@example.com",
                "uuid-1",
                Some("old_rt"),
                "old_at",
            );
            acc.name = Some("business".to_string());
            acc.last_error = Some(
                "401 Unauthorized (Session ended (logged out in app). Re-login required.)"
                    .to_string(),
            );
            acc
        }],
    };

    let jwt = make_test_jwt("test-user@example.com");
    let new_tokens = AuthTokens {
        access_token: jwt.clone(),
        refresh_token: Some("new_rt".to_string()),
        id_token: Some(jwt),
        account_id: Some("uuid-1".to_string()),
    };

    let res = apply_relogin_to_accounts_file(&mut file, "business", new_tokens);
    assert!(res.is_ok());
    assert_eq!(res.unwrap(), "test-user@example.com:uuid-1");
    assert_eq!(file.accounts.len(), 1);
    assert_eq!(file.accounts[0].last_error, None);
    assert_eq!(
        file.accounts[0].tokens.refresh_token.as_deref(),
        Some("new_rt")
    );
    assert!(file.accounts[0].enabled);

    let _ = std::fs::remove_dir_all(&temp_dir);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn test_apply_relogin_rejects_email_mismatch() {
    let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir = std::env::temp_dir().join(format!(
        "codex_relogin_mismatch_test_{}",
        std::process::id()
    ));
    let _ = std::fs::create_dir_all(&temp_dir);
    std::env::set_var("CODEX_HOME", &temp_dir);

    let mut file = AccountsFile {
        active_account_id: Some("test-user@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![{
            let mut acc = make_test_account(
                "test-user@example.com:uuid-1",
                "test-user@example.com",
                "uuid-1",
                Some("old_rt"),
                "old_at",
            );
            acc.name = Some("business".to_string());
            acc.last_error = Some("401 Unauthorized".to_string());
            acc
        }],
    };

    // Browser logged into different account "other@example.com"
    let jwt = make_test_jwt("other@example.com");
    let new_tokens = AuthTokens {
        access_token: jwt.clone(),
        refresh_token: Some("other_rt".to_string()),
        id_token: Some(jwt),
        account_id: Some("uuid-2".to_string()),
    };

    let res = apply_relogin_to_accounts_file(&mut file, "business", new_tokens);
    assert!(res.is_err());
    let err_msg = res.unwrap_err();
    assert!(err_msg.contains("Logged in as 'other@example.com'"));
    assert!(err_msg.contains("expected 'test-user@example.com'"));

    // Existing account must remain untouched
    assert_eq!(
        file.accounts[0].tokens.refresh_token.as_deref(),
        Some("old_rt")
    );
    assert_eq!(
        file.accounts[0].last_error,
        Some("401 Unauthorized".to_string())
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn test_apply_relogin_syncs_active_auth_json() {
    let _lock = TEST_CODEX_HOME_MUTEX.lock().unwrap();
    let temp_dir =
        std::env::temp_dir().join(format!("codex_relogin_sync_test_{}", std::process::id()));
    let _ = std::fs::create_dir_all(&temp_dir);
    std::env::set_var("CODEX_HOME", &temp_dir);

    let initial_auth = AuthJson {
        auth_mode: Some("chatgpt".to_string()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: "old_active_at".to_string(),
            refresh_token: Some("old_active_rt".to_string()),
            id_token: None,
            account_id: Some("uuid-1".to_string()),
        }),
        last_refresh: None,
    };
    crate::storage::write_active_auth_json(&initial_auth).unwrap();

    let mut file = AccountsFile {
        active_account_id: Some("active@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "active@example.com:uuid-1",
            "active@example.com",
            "uuid-1",
            Some("old_active_rt"),
            "old_active_at",
        )],
    };

    let jwt = make_test_jwt("active@example.com");
    let new_tokens = AuthTokens {
        access_token: jwt.clone(),
        refresh_token: Some("new_active_rt".to_string()),
        id_token: Some(jwt),
        account_id: Some("uuid-1".to_string()),
    };

    let res = apply_relogin_to_accounts_file(&mut file, "active@example.com:uuid-1", new_tokens);
    assert!(res.is_ok());

    let active_auth = crate::storage::read_active_auth_json().unwrap();
    assert_eq!(
        active_auth.tokens.unwrap().refresh_token.as_deref(),
        Some("new_active_rt")
    );

    let _ = std::fs::remove_dir_all(&temp_dir);
    std::env::remove_var("CODEX_HOME");
}

#[test]
fn test_reset_account_in_file_success() {
    let mut file = crate::models::AccountsFile {
        active_account_id: Some("user1@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "user1@example.com:uuid-1",
            "user1@example.com",
            "uuid-1",
            Some("rt_1"),
            "at_1",
        )],
    };
    file.accounts[0].last_credits = Some(2);

    let result = reset_account_in_file(&mut file, "user1@example.com", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::Applied
    });

    assert!(result.is_ok());
    let (name, is_active) = result.unwrap();
    assert_eq!(name, "user1");
    assert!(is_active);
    assert_eq!(file.accounts[0].last_credits, Some(1));
}

#[test]
fn test_reset_account_in_file_no_credits() {
    let mut file = crate::models::AccountsFile {
        active_account_id: Some("user1@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "user1@example.com:uuid-1",
            "user1@example.com",
            "uuid-1",
            Some("rt_1"),
            "at_1",
        )],
    };
    file.accounts[0].last_credits = Some(0);

    let result = reset_account_in_file(&mut file, "user1@example.com", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::Applied
    });

    assert!(result.is_err());
    assert!(result.unwrap_err().contains("no reset credits available"));
    assert_eq!(file.accounts[0].last_credits, Some(0));
}

#[test]
fn test_reset_account_in_file_outcome_handling() {
    let mut file = crate::models::AccountsFile {
        active_account_id: Some("user1@example.com:uuid-1".to_string()),
        settings: Default::default(),
        accounts: vec![make_test_account(
            "user1@example.com:uuid-1",
            "user1@example.com",
            "uuid-1",
            Some("rt_1"),
            "at_1",
        )],
    };
    file.accounts[0].last_credits = Some(1);

    // NotConsumed
    let res1 = reset_account_in_file(&mut file, "user1@example.com", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into())
    });
    assert!(res1.is_err());
    assert!(res1.unwrap_err().contains("Reset credit was not consumed"));
    assert_eq!(file.accounts[0].last_credits, Some(1));

    // Unavailable
    let res2 = reset_account_in_file(&mut file, "user1@example.com", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::Unavailable("service_busy".into())
    });
    assert!(res2.is_err());
    assert!(res2.unwrap_err().contains("service is unavailable"));
    assert_eq!(file.accounts[0].last_credits, Some(1));
}

#[test]
fn test_reset_account_in_file_desktop_app_alias() {
    let mut file = crate::models::AccountsFile {
        active_account_id: Some("user2@example.com:uuid-2".to_string()),
        settings: Default::default(),
        accounts: vec![
            make_test_account(
                "user1@example.com:uuid-1",
                "user1@example.com",
                "uuid-1",
                Some("rt_1"),
                "at_1",
            ),
            make_test_account(
                "user2@example.com:uuid-2",
                "user2@example.com",
                "uuid-2",
                Some("rt_2"),
                "at_2",
            ),
        ],
    };
    file.accounts[0].last_credits = Some(1);
    file.accounts[1].last_credits = Some(2);

    // "desktop-app" should resolve to active account (user2)
    let res = reset_account_in_file(&mut file, "desktop-app", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::Applied
    });
    assert!(res.is_ok());
    let (name, is_active) = res.unwrap();
    assert_eq!(name, "user2");
    assert!(is_active);
    assert_eq!(file.accounts[1].last_credits, Some(1));

    // "active" should also resolve to active account
    let res_active = reset_account_in_file(&mut file, "active", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::Applied
    });
    assert!(res_active.is_ok());
    assert_eq!(file.accounts[1].last_credits, Some(0));

    // empty query should return error
    let res_empty = reset_account_in_file(&mut file, "   ", |_acc, _idemp| {
        crate::quota::ResetCreditConsumeOutcome::Applied
    });
    assert!(res_empty.is_err());
}
