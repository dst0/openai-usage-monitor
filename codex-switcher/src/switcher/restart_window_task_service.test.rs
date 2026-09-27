use super::RestartWindowTaskService;
use crate::distribution::WindowProcessIdentity;

/// Without the request nothing is resolved: no helper, no Codex home, no
/// window inventory. Test builds panic on any of those, so reaching this
/// assertion proves the unrequested path stays inert.
#[test]
fn an_unrequested_restart_touches_no_window() {
    let expected = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let mut session = RestartWindowTaskService::capture_if_requested(false, &expected).unwrap();
    assert!(session.is_none());
    assert_eq!(RestartWindowTaskService::captured_windows(&session), None);
    RestartWindowTaskService::restore(&mut session, 5151, "after relaunch");
    assert_eq!(RestartWindowTaskService::finish(session), Ok(()));
}

#[test]
fn window_failures_are_added_to_other_restart_failures() {
    assert_eq!(RestartWindowTaskService::append_failure(None, None), None);
    assert_eq!(
        RestartWindowTaskService::append_failure(Some("recovery failed".into()), None),
        Some("recovery failed".into())
    );
}
