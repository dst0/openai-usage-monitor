use super::*;

#[test]
fn external_relaunch_rebinds_only_the_same_account_present_before_process_birth() {
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let candidate = DesktopExternalBindingService::candidate(
        &old_session(),
        &process,
        "account-a",
        "current-auth-file",
        moment(1999),
        moment(1998),
        moment(2001),
    )
    .expect("a verified pre-launch account should rebind the new process");

    assert_eq!(candidate.account_id, "account-a");
    assert_eq!(candidate.cli_account_id.as_deref(), Some("account-a"));
    assert_eq!(candidate.process, Some(process));
    assert_eq!(candidate.auth_file_id.as_deref(), Some("current-auth-file"));
}

#[test]
fn changed_account_or_post_launch_auth_never_rebinds() {
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    for (account, auth_modified) in [("account-b", moment(1999)), ("account-a", moment(2001))] {
        assert!(DesktopExternalBindingService::candidate(
            &old_session(),
            &process,
            account,
            "current-auth-file",
            auth_modified,
            moment(1998),
            moment(2002)
        )
        .is_none());
    }
}

#[test]
fn post_launch_file_restore_with_backdated_mtime_never_rebinds() {
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    assert!(DesktopExternalBindingService::candidate(
        &old_session(),
        &process,
        "account-a",
        "restored-auth-file",
        moment(1999),
        moment(2001),
        moment(2002),
    )
    .is_none());
}

#[test]
fn legacy_or_current_process_marker_never_bootstraps() {
    let process = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let mut legacy = old_session();
    legacy.process = None;
    assert!(DesktopExternalBindingService::candidate(
        &legacy,
        &process,
        "account-a",
        "current-auth-file",
        moment(1999),
        moment(1998),
        moment(2001)
    )
    .is_none());

    let mut current = old_session();
    current.process = Some(process.clone());
    assert!(DesktopExternalBindingService::candidate(
        &current,
        &process,
        "account-a",
        "current-auth-file",
        moment(1999),
        moment(1998),
        moment(2001)
    )
    .is_none());
}

#[test]
fn invalid_marker_and_process_time_boundaries_never_bootstrap() {
    let current = WindowProcessIdentity::new(200, "2000:000000").unwrap();
    let cases = [
        (
            "1970-01-01T00:16:39Z",
            "1000:000000",
            moment(1999),
            moment(2001),
        ),
        (
            "1970-01-01T00:33:20Z",
            "1000:000000",
            moment(1999),
            moment(2001),
        ),
        (
            "1970-01-01T00:16:41Z",
            "2001:000000",
            moment(1999),
            moment(2001),
        ),
        (
            "1970-01-01T00:16:41Z",
            "invalid",
            moment(1999),
            moment(2001),
        ),
        (
            "1970-01-01T00:16:41Z",
            "1000:000000",
            moment(2000),
            moment(2001),
        ),
        (
            "1970-01-01T00:16:41Z",
            "1000:000000",
            moment(1999),
            moment(1999),
        ),
    ];
    for (updated_at, old_birth, auth_modified, now) in cases {
        let mut previous = old_session();
        previous.updated_at = updated_at.into();
        previous.process = Some(WindowProcessIdentity::new(100, old_birth).unwrap());
        assert!(DesktopExternalBindingService::candidate(
            &previous,
            &current,
            "account-a",
            "current-auth-file",
            auth_modified,
            moment(1998),
            now,
        )
        .is_none());
    }
    let malformed_current = WindowProcessIdentity::new(200, "2000:1000000").unwrap();
    assert!(DesktopExternalBindingService::candidate(
        &old_session(),
        &malformed_current,
        "account-a",
        "current-auth-file",
        moment(1999),
        moment(1998),
        moment(2001),
    )
    .is_none());
}
