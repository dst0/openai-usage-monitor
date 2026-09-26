use super::{is_identity_change, LaunchSpec};

#[test]
fn launch_services_url_spec_matches_packed_macos_abi() {
    assert_eq!(std::mem::size_of::<LaunchSpec>(), 36);
    assert_eq!(std::mem::offset_of!(LaunchSpec, app_url), 0);
    assert_eq!(std::mem::offset_of!(LaunchSpec, item_urls), 8);
    assert_eq!(std::mem::offset_of!(LaunchSpec, pass_thru_params), 16);
    assert_eq!(std::mem::offset_of!(LaunchSpec, launch_flags), 24);
    assert_eq!(std::mem::offset_of!(LaunchSpec, async_ref_con), 28);
}

#[test]
fn only_changed_process_identity_is_a_fatal_navigation_error() {
    assert!(is_identity_change(
        "ChatGPT process identity changed during task navigation"
    ));
    assert!(!is_identity_change(
        "Pinned ChatGPT task navigation failed (OSStatus -10814)"
    ));
}

#[test]
fn unit_tests_cannot_send_a_pinned_task_link() {
    // An invalid ID: with the tripwire removed this returns a validation
    // error before any LaunchServices, bundle, or process access.
    crate::test_live_system::assert_forbidden("pinned ChatGPT task link (LaunchServices)", || {
        super::retry("not-a-thread")
    });
}
