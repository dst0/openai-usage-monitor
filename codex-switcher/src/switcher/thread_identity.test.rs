use super::thread_open_command;

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
