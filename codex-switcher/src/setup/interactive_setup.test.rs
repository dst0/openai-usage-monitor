use super::{
    load_login_accounts_with, login_and_add_account_with_codex_bin,
    preserve_existing_active_session_with,
};
use crate::models::{AccountsFile, AuthJson, AuthTokens};

fn synthetic_auth() -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(AuthTokens {
            access_token: "synthetic-access".into(),
            refresh_token: Some("synthetic-refresh".into()),
            id_token: None,
            account_id: Some("synthetic-workspace".into()),
            extra: Default::default(),
        }),
        last_refresh: None,
        extra: Default::default(),
    }
}

#[test]
fn interactive_login_rejects_invalid_registry_instead_of_assuming_empty() {
    assert!(load_login_accounts_with(|| Err("invalid registry".into())).is_err());
}

#[test]
fn interactive_login_rejects_failed_active_session_save() {
    let mut accounts = AccountsFile::default();
    let result = preserve_existing_active_session_with(
        &mut accounts,
        || Ok(Some(synthetic_auth())),
        |_, _| Err("synthetic save failure".into()),
    );
    assert!(result.is_err());
}

#[test]
fn interactive_login_rejects_unreadable_existing_active_auth() {
    let mut accounts = AccountsFile::default();
    let result = preserve_existing_active_session_with(
        &mut accounts,
        || Err("synthetic read failure".into()),
        |_, _| panic!("unreadable active auth must not be replaced"),
    );
    assert!(result.is_err());
}

#[test]
fn interactive_login_never_removes_a_preexisting_pid_directory() {
    let legacy = std::env::temp_dir().join(format!("codex-login-{}", std::process::id()));
    std::fs::create_dir(&legacy).expect("legacy PID path must not exist before this test");
    let sentinel = legacy.join("keep-this-file");
    std::fs::write(&sentinel, b"synthetic data").unwrap();

    let result = login_and_add_account_with_codex_bin("synthetic", "/usr/bin/false");
    let preserved = sentinel.exists();
    if preserved {
        std::fs::remove_file(&sentinel).unwrap();
    }
    if legacy.is_dir() {
        std::fs::remove_dir(&legacy).unwrap();
    }
    assert!(result.is_err());
    assert!(preserved, "existing temporary directory was removed");
}
