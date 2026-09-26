use super::{retry_thread_link_natively_in_background, thread_open_command};

#[test]
fn cold_task_open_brings_chatgpt_to_foreground_for_mounting() {
    let command = thread_open_command("01234567-89ab-cdef-0123-456789abcdef", true);
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert_eq!(
        args,
        [
            "-a",
            "/Applications/ChatGPT.app",
            "codex://threads/01234567-89ab-cdef-0123-456789abcdef",
        ]
    );
}

#[test]
fn retry_does_not_keep_stealing_focus() {
    let command = thread_open_command("01234567-89ab-cdef-0123-456789abcdef", false);
    let args: Vec<_> = command
        .get_args()
        .map(|arg| arg.to_string_lossy())
        .collect();
    assert_eq!(args.first().map(|arg| arg.as_ref()), Some("-g"));
}

#[test]
fn pinned_retry_rejects_invalid_id_before_any_desktop_access() {
    assert_eq!(
        retry_thread_link_natively_in_background("not-a-thread").unwrap_err(),
        "Invalid thread ID for ChatGPT navigation"
    );
}

#[test]
fn unit_tests_cannot_open_chatgpt_task_links() {
    // An IPC fake that answers `no-client-found` leads recovery here. The
    // tripwire precedes validation, so a removed tripwire fails this test on
    // the invalid ID instead of launching ChatGPT.
    let seam = "ChatGPT task link (/usr/bin/open)";
    crate::test_live_system::assert_forbidden(seam, || super::open_thread_in_codex("not-a-thread"));
    crate::test_live_system::assert_forbidden(seam, || {
        super::retry_thread_link_in_background("not-a-thread")
    });
}
