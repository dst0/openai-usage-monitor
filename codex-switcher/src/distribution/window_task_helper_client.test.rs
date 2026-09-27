use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const BIRTH: &str = "1726789012:000007";

/// A temporary fake helper; nothing here resolves the installed helper.
struct FakeHelper {
    root: PathBuf,
}

impl FakeHelper {
    fn new(label: &str, body: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codex-window-task-client-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(&root).unwrap();
        let helper = root.join("window-helper");
        // Records its argv one per line and its stdin, then runs `body`.
        let script = format!(
            "#!/bin/sh\nfor argument in \"$@\"; do printf '%s\\n' \"$argument\"; done > '{args}'\ncat > '{stdin}'\n{body}\n",
            args = root.join("args").display(),
            stdin = root.join("stdin").display(),
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { root }
    }

    fn backend(&self) -> SystemWindowRestoreBackend {
        SystemWindowRestoreBackend::with_helper(self.root.join("window-helper"))
    }

    fn args(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("args"))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    fn stdin(&self) -> Vec<u8> {
        std::fs::read(self.root.join("stdin")).unwrap()
    }
}

impl Drop for FakeHelper {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn process() -> ProcessIdentity {
    ProcessIdentity::new(4242, BIRTH).unwrap()
}

#[test]
fn every_command_passes_the_exact_process_and_explicit_permission() {
    for command in [
        WindowTaskCommand::Probe,
        WindowTaskCommand::Snapshot,
        WindowTaskCommand::Restore,
        WindowTaskCommand::Rehearse,
    ] {
        let helper = FakeHelper::new("args", "printf '{\"ok\":true}'");
        let backend = helper.backend();
        let response = WindowTaskHelperClient::new(&backend)
            .run(command, &process(), None)
            .unwrap();
        assert_eq!(response, serde_json::json!({"ok": true}));
        assert_eq!(
            helper.args(),
            [
                command.helper_command(),
                "--expected-pid",
                "4242",
                "--expected-birth",
                BIRTH,
                "--allow-focus-and-clipboard",
                "yes",
            ]
        );
        // Without input the helper's stdin is empty, never inherited.
        assert!(helper.stdin().is_empty());
    }
}

#[test]
fn a_plan_goes_through_stdin_and_never_through_argv() {
    let helper = FakeHelper::new("stdin", "printf '{}'");
    let backend = helper.backend();
    let plan = br#"{"windows":[{"task_id":"01a00000-0000-4000-8000-00000000000a"}]}"#;
    WindowTaskHelperClient::new(&backend)
        .run(WindowTaskCommand::Restore, &process(), Some(plan))
        .unwrap();
    assert_eq!(helper.stdin(), plan);
    assert!(helper
        .args()
        .iter()
        .all(|argument| !argument.contains("01a00000")));
}

#[test]
fn an_oversized_plan_is_refused_before_the_helper_starts() {
    let helper = FakeHelper::new("oversized", "printf '{}'");
    let backend = helper.backend();
    let plan = vec![b' '; MAX_INPUT_BYTES + 1];
    assert_eq!(
        WindowTaskHelperClient::new(&backend).run(
            WindowTaskCommand::Restore,
            &process(),
            Some(&plan)
        ),
        Err("Window task restore plan is too large".into())
    );
    assert!(!helper.root.join("args").exists());
}

#[test]
fn failures_are_named_by_command_without_echoing_helper_output() {
    let helper = FakeHelper::new(
        "failure",
        "printf 'TASK_NAVIGATION_FAILED after-focus\\n' >&2; exit 1",
    );
    let backend = helper.backend();
    let client = WindowTaskHelperClient::new(&backend);
    assert_eq!(
        client.run(WindowTaskCommand::Restore, &process(), None),
        Err(
            "Window task restore failed: TASK_NAVIGATION_FAILED after it began focusing \
             ChatGPT windows; the clipboard may now hold a copied task link"
                .into()
        )
    );
    let leaky = FakeHelper::new(
        "leaky",
        "printf 'codex://threads/01a00000-0000-4000-8000-00000000000a\\n' >&2; exit 1",
    );
    let backend = leaky.backend();
    assert_eq!(
        WindowTaskHelperClient::new(&backend).run(WindowTaskCommand::Snapshot, &process(), None),
        Err(HELPER_REJECTED.into())
    );
    let garbage = FakeHelper::new("garbage", "printf 'not json'");
    let backend = garbage.backend();
    assert_eq!(
        WindowTaskHelperClient::new(&backend).run(WindowTaskCommand::Probe, &process(), None),
        Err("Codex window restore helper returned invalid data".into())
    );
    let missing = SystemWindowRestoreBackend::with_helper(PathBuf::from("/nonexistent/helper"));
    assert_eq!(
        WindowTaskHelperClient::new(&missing).run(WindowTaskCommand::Probe, &process(), None),
        Err("Codex window restore helper could not start".into())
    );
}
