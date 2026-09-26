use super::*;

#[test]
fn linked_or_malformed_session_marker_blocks_external_rebinding() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-marker-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&home).unwrap();
    let marker_path = home.join("desktop-app-session.json");
    let real_marker = home.join("real-session.json");
    old_session().save(&real_marker).unwrap();
    symlink(&real_marker, &marker_path).unwrap();
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let check = || {
        DesktopExternalBindingService::refresh_with(
            &home,
            moment(2001),
            || vec![200],
            |_| Ok(process.clone()),
            || {
                Some((
                    "account-a".into(),
                    "original-auth".into(),
                    moment(1999),
                    moment(1998),
                ))
            },
        )
    };
    assert!(check().is_err());
    std::fs::remove_file(&marker_path).unwrap();
    std::fs::write(&marker_path, b"invalid").unwrap();
    std::fs::set_permissions(&marker_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert!(check().is_err());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn non_private_or_linked_auth_never_provides_desktop_identity() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-private-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&home).unwrap();
    let account = make_account(
        "account-a",
        None,
        "a@example.com",
        "pro",
        50.0,
        None,
        0,
        None,
        None,
    );
    private_json(
        &home.join("accounts.json"),
        &AccountsFile {
            active_account_id: Some(account.id.clone()),
            settings: Settings::default(),
            accounts: vec![account.clone()],
        },
    );
    let real_auth = home.join("real-auth.json");
    private_json(
        &real_auth,
        &AuthJson {
            auth_mode: Some("chatgpt".into()),
            openai_api_key: None,
            tokens: Some(account.tokens),
            last_refresh: None,
            extra: Default::default(),
        },
    );
    symlink(&real_auth, home.join("auth.json")).unwrap();
    assert!(DesktopExternalBindingService::read_auth_evidence(&home).is_none());
    std::fs::remove_file(home.join("auth.json")).unwrap();
    std::fs::rename(&real_auth, home.join("auth.json")).unwrap();
    std::fs::set_permissions(
        home.join("auth.json"),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    assert!(DesktopExternalBindingService::read_auth_evidence(&home).is_none());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn conflicting_jwt_email_cannot_rebind_app_even_when_tokens_match_registry() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-identity-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&home).unwrap();
    let mut account = make_account(
        "account-a",
        None,
        "a@example.com",
        "pro",
        50.0,
        None,
        0,
        None,
        None,
    );
    let payload = URL_SAFE_NO_PAD.encode(r#"{"email":"other@example.com"}"#);
    account.tokens.access_token = format!("header.{payload}.signature");
    private_json(
        &home.join("accounts.json"),
        &AccountsFile {
            active_account_id: Some(account.id.clone()),
            settings: Settings::default(),
            accounts: vec![account.clone()],
        },
    );
    private_json(
        &home.join("auth.json"),
        &AuthJson {
            auth_mode: Some("chatgpt".into()),
            openai_api_key: None,
            tokens: Some(account.tokens),
            last_refresh: None,
            extra: Default::default(),
        },
    );
    assert!(DesktopExternalBindingService::read_auth_evidence(&home).is_none());
    std::fs::remove_dir_all(home).unwrap();
}
