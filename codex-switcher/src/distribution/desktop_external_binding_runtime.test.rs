use super::*;

#[test]
fn external_relaunch_updates_the_real_marker_from_private_auth_and_registry() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-{}-{}",
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
    let auth_path = home.join("auth.json");
    std::fs::set_permissions(&auth_path, std::fs::Permissions::from_mode(0o700)).unwrap();
    std::fs::set_permissions(&auth_path, std::fs::Permissions::from_mode(0o600)).unwrap();
    old_session()
        .save(&home.join("desktop-app-session.json"))
        .unwrap();
    std::thread::sleep(Duration::from_secs(2));
    let birth = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let process = WindowProcessIdentity::new(200, format!("{birth}:000000")).unwrap();
    let now = SystemTime::now();
    let changed = DesktopExternalBindingService::refresh_with(
        &home,
        now,
        || vec![200],
        |_| Ok(process.clone()),
        || DesktopExternalBindingService::read_auth_evidence(&home),
    )
    .unwrap();
    assert!(changed);
    let marker = DesktopAppSession::load(&home.join("desktop-app-session.json")).unwrap();
    assert_eq!(marker.process, Some(process));
    assert_eq!(marker.account_id, "account-a");
    assert!(marker.matches_process_lifetime(&format!("{birth}:000000")));
    assert!(marker
        .auth_file_id
        .as_ref()
        .is_some_and(|id| !id.is_empty()));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn restored_auth_with_backdated_mtime_cannot_impersonate_prelaunch_auth() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-restored-{}-{}",
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
    let auth_path = home.join("auth.json");
    private_json(
        &auth_path,
        &AuthJson {
            auth_mode: Some("chatgpt".into()),
            openai_api_key: None,
            tokens: Some(account.tokens),
            last_refresh: None,
            extra: Default::default(),
        },
    );
    let now = SystemTime::now();
    let old = now.duration_since(UNIX_EPOCH).unwrap().as_secs() as libc::time_t - 120;
    let times = [
        libc::timeval {
            tv_sec: old,
            tv_usec: 0,
        },
        libc::timeval {
            tv_sec: old,
            tv_usec: 0,
        },
    ];
    let path = CString::new(auth_path.as_os_str().as_bytes()).unwrap();
    // SAFETY: the path and two timeval values are valid for this synchronous call.
    assert_eq!(unsafe { libc::utimes(path.as_ptr(), times.as_ptr()) }, 0);
    let marker_path = home.join("desktop-app-session.json");
    let previous = old_session();
    previous.save(&marker_path).unwrap();
    let birth = now.duration_since(UNIX_EPOCH).unwrap().as_secs() - 60;
    let process = WindowProcessIdentity::new(200, format!("{birth}:000000")).unwrap();
    assert!(!DesktopExternalBindingService::refresh_with(
        &home,
        now,
        || vec![200],
        |_| Ok(process.clone()),
        || DesktopExternalBindingService::read_auth_evidence(&home),
    )
    .unwrap());
    assert_eq!(DesktopAppSession::load(&marker_path), Some(previous));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn auth_change_after_marker_save_restores_the_previous_unknown_binding() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-race-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&home).unwrap();
    let path = home.join("desktop-app-session.json");
    let previous = old_session();
    previous.save(&path).unwrap();
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let mut auth_reads = 0;
    let changed = DesktopExternalBindingService::refresh_with(
        &home,
        moment(2001),
        || vec![200],
        |_| Ok(process.clone()),
        || {
            auth_reads += 1;
            let file_id = if auth_reads == 3 {
                "replaced-auth"
            } else {
                "original-auth"
            };
            Some((
                "account-a".into(),
                file_id.into(),
                moment(1999),
                moment(1998),
            ))
        },
    )
    .unwrap();
    assert!(!changed);
    assert_eq!(DesktopAppSession::load(&path), Some(previous));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn process_change_after_marker_save_restores_the_previous_unknown_binding() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-process-race-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&home).unwrap();
    let marker_path = home.join("desktop-app-session.json");
    let previous = old_session();
    previous.save(&marker_path).unwrap();
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let mut pid_reads = 0;
    let changed = DesktopExternalBindingService::refresh_with(
        &home,
        moment(2001),
        || {
            pid_reads += 1;
            if pid_reads == 3 {
                vec![201]
            } else {
                vec![200]
            }
        },
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
    .unwrap();
    assert!(!changed);
    assert_eq!(DesktopAppSession::load(&marker_path), Some(previous));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn process_inspection_failure_after_save_restores_the_previous_marker() {
    let home = std::env::temp_dir().join(format!(
        "codex-external-binding-inspect-race-{}-{}",
        std::process::id(),
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&home).unwrap();
    let marker_path = home.join("desktop-app-session.json");
    let previous = old_session();
    previous.save(&marker_path).unwrap();
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let mut inspections = 0;
    let result = DesktopExternalBindingService::refresh_with(
        &home,
        moment(2001),
        || vec![200],
        |_| {
            inspections += 1;
            if inspections == 3 {
                Err("helper unavailable".into())
            } else {
                Ok(process.clone())
            }
        },
        || {
            Some((
                "account-a".into(),
                "original-auth".into(),
                moment(1999),
                moment(1998),
            ))
        },
    );
    assert!(result.is_ok());
    assert!(!result.unwrap());
    assert_eq!(DesktopAppSession::load(&marker_path), Some(previous));
    std::fs::remove_dir_all(home).unwrap();
}
