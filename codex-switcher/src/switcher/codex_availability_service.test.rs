use super::CodexAvailabilityService;
use std::cell::Cell;

#[test]
fn resume_does_not_launch_a_running_desktop() {
    let launched = Cell::new(false);
    CodexAvailabilityService::ensure_running_for_resume(
        || true,
        || {
            launched.set(true);
            Ok(vec![1])
        },
    )
    .unwrap();
    assert!(!launched.get());
}

#[test]
fn resume_launches_a_closed_desktop_once() {
    let launches = Cell::new(0);
    CodexAvailabilityService::ensure_running_for_resume(
        || false,
        || {
            launches.set(launches.get() + 1);
            Ok(vec![4242])
        },
    )
    .unwrap();
    assert_eq!(launches.get(), 1);
}

#[test]
fn resume_launch_failure_explains_how_to_start_desktop() {
    let error = CodexAvailabilityService::ensure_running_for_resume(
        || false,
        || Err("Codex app launch was requested, but no stable main process appeared".into()),
    )
    .unwrap_err();
    assert!(error.starts_with("Codex is not running and could not be started automatically"));
    assert!(error.contains("no stable main process appeared"));
    assert!(error.contains("open -a /Applications/ChatGPT.app"));
    assert!(error.contains("cxi resume"));
}

#[test]
fn resume_starts_desktop_only_for_an_eligible_user_task() {
    let user_task = "01a0d8a1-f49c-7cd1-8e5f-cbf327837b5d".to_string();
    let is_user = |id: &str| id == "01a0d8a1-f49c-7cd1-8e5f-cbf327837b5d";
    assert!(CodexAvailabilityService::resume_has_launchable_target(
        std::slice::from_ref(&user_task),
        is_user
    ));
    assert!(!CodexAvailabilityService::resume_has_launchable_target(
        &["garbage".to_string()],
        is_user
    ));
    assert!(!CodexAvailabilityService::resume_has_launchable_target(
        &[],
        is_user
    ));
}
