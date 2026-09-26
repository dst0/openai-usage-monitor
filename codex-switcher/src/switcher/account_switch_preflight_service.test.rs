use super::AccountSwitchPreflightService;
use crate::{
    distribution::test_account_spec::TestAccountSpec,
    models::{AccountConfig, AuthJson},
};
use std::cell::Cell;

fn account(error: Option<&str>) -> AccountConfig {
    TestAccountSpec {
        id: "target-account",
        name: Some("work"),
        email: "target@example.test",
        plan: "team",
        sprint_pct: 80.0,
        error,
        ..TestAccountSpec::default()
    }
    .build()
}

fn auth() -> AuthJson {
    AuthJson {
        auth_mode: Some("chatgpt".into()),
        openai_api_key: None,
        tokens: Some(account(None).tokens),
        last_refresh: None,
        extra: Default::default(),
    }
}

fn check(
    target: &AccountConfig,
    auth: Option<&AuthJson>,
    desktop_running: Result<bool, &str>,
    shared_auth_active: Result<bool, &str>,
) -> Result<bool, String> {
    AccountSwitchPreflightService::check(
        target,
        auth,
        || desktop_running.map_err(str::to_owned),
        || shared_auth_active.map_err(str::to_owned),
    )
}

#[test]
fn relogin_target_is_refused_whatever_desktop_is_doing() {
    let expired = account(Some("401 Unauthorized (Session ended)"));
    for (running, auth) in [(false, None), (true, Some(auth()))] {
        let error = check(&expired, auth.as_ref(), Ok(running), Ok(running)).unwrap_err();
        assert!(error.contains("requires re-login"), "{error}");
        assert!(error.contains("cxi relogin \"work\""), "{error}");
    }
}

#[test]
fn orphaned_credential_writer_is_refused_before_target_checks() {
    let expired = account(Some("401 Unauthorized"));
    let error = check(&expired, Some(&auth()), Ok(false), Ok(true)).unwrap_err();
    assert_eq!(
        error,
        "A bundled Desktop credential writer is running without its main process"
    );
}

#[test]
fn running_desktop_without_readable_auth_is_refused() {
    let error = check(&account(None), None, Ok(true), Ok(true)).unwrap_err();
    assert_eq!(error, "Running Desktop has no readable authentication");
}

#[test]
fn process_probe_failures_block_the_switch() {
    let shared_probed = Cell::new(false);
    let error = AccountSwitchPreflightService::check(
        &account(None),
        Some(&auth()),
        || Err("Desktop process inspection failed".into()),
        || {
            shared_probed.set(true);
            Ok(false)
        },
    )
    .unwrap_err();
    assert_eq!(error, "Desktop process inspection failed");
    assert!(!shared_probed.get());
    assert_eq!(
        check(&account(None), Some(&auth()), Ok(true), Err("ps exited 1")).unwrap_err(),
        "ps exited 1"
    );
}

#[test]
fn eligible_target_reports_whether_desktop_is_running() {
    assert!(check(&account(None), Some(&auth()), Ok(true), Ok(true)).unwrap());
    assert!(!check(&account(None), None, Ok(false), Ok(false)).unwrap());
}
