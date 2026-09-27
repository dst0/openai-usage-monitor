use super::*;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const BIRTH: &str = "1726789012:000007";
const WAIT: Duration = Duration::from_secs(20);

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
        // Records its argv one per line, whether stdin is /dev/null, and its
        // stdin, then runs `body`.
        let script = format!(
            "#!/bin/sh\nfor argument in \"$@\"; do printf '%s\\n' \"$argument\"; done > '{args}'\n[ \"$(/usr/bin/stat -L -f %d:%i /dev/stdin)\" = \"$(/usr/bin/stat -L -f %d:%i /dev/null)\" ] && : > '{null}'\ncat > '{stdin}'\n{body}\n",
            args = root.join("args").display(),
            null = root.join("stdin-is-null").display(),
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
            .run(command, &process(), None, WAIT)
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
        // Without input the helper's stdin is /dev/null, never inherited.
        assert!(helper.stdin().is_empty());
        assert!(helper.root.join("stdin-is-null").exists());
    }
}

#[test]
fn a_plan_goes_through_stdin_and_never_through_argv() {
    let helper = FakeHelper::new("stdin", "printf '{}'");
    let backend = helper.backend();
    let plan = br#"{"windows":[{"task_id":"01a00000-0000-4000-8000-00000000000a"}]}"#;
    WindowTaskHelperClient::new(&backend)
        .run(WindowTaskCommand::Restore, &process(), Some(plan), WAIT)
        .unwrap();
    assert_eq!(helper.stdin(), plan);
    assert!(!helper.root.join("stdin-is-null").exists());
    // A plan exactly at the limit is still sent.
    let largest = vec![b' '; MAX_INPUT_BYTES];
    WindowTaskHelperClient::new(&backend)
        .run(WindowTaskCommand::Restore, &process(), Some(&largest), WAIT)
        .unwrap();
    assert_eq!(helper.stdin().len(), MAX_INPUT_BYTES);
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
            Some(&plan),
            WAIT
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
        client.run(WindowTaskCommand::Restore, &process(), None, WAIT),
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
        WindowTaskHelperClient::new(&backend).run(
            WindowTaskCommand::Snapshot,
            &process(),
            None,
            WAIT
        ),
        Err(HELPER_REJECTED.into())
    );
    let garbage = FakeHelper::new("garbage", "printf 'not json'");
    let backend = garbage.backend();
    assert_eq!(
        WindowTaskHelperClient::new(&backend).run(WindowTaskCommand::Probe, &process(), None, WAIT),
        Err("Codex window restore helper returned invalid data".into())
    );
    let missing = SystemWindowRestoreBackend::with_helper(PathBuf::from("/nonexistent/helper"));
    assert_eq!(
        WindowTaskHelperClient::new(&missing).run(WindowTaskCommand::Probe, &process(), None, WAIT),
        Err("Codex window restore helper could not start".into())
    );
}

fn alive(pid: libc::pid_t) -> bool {
    // SAFETY: signal 0 only checks that the process exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// A helper that never answers, with a descendant holding its output pipe
/// open, is stopped at the deadline together with that descendant.
#[test]
fn a_stuck_helper_and_its_descendants_are_stopped_at_the_deadline() {
    let helper = FakeHelper::new("stuck", "");
    let child_pid = helper.root.join("child-pid");
    let script = format!(
        "#!/bin/sh\n/bin/sleep 60 &\nprintf '%s' \"$!\" > '{}'\nexec /bin/sleep 60\n",
        child_pid.display()
    );
    std::fs::write(helper.root.join("window-helper"), script).unwrap();
    let backend = helper.backend();
    let started = Instant::now();
    let result = WindowTaskHelperClient::new(&backend).run(
        WindowTaskCommand::Restore,
        &process(),
        Some(b"{}"),
        Duration::from_secs(1),
    );
    assert!(
        started.elapsed() < Duration::from_secs(8),
        "{:?}",
        started.elapsed()
    );
    let error = result.unwrap_err();
    assert!(
        error.starts_with("Window task restore timed out after 1s"),
        "{error}"
    );
    let pid: libc::pid_t = std::fs::read_to_string(&child_pid)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    let deadline = Instant::now() + Duration::from_secs(5);
    while alive(pid) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(50));
    }
    assert!(!alive(pid), "the helper's descendant survived the deadline");
}

#[test]
fn deadlines_grow_with_the_window_count() {
    assert_eq!(
        WindowTaskCommand::Snapshot.timeout(2),
        Duration::from_secs(40)
    );
    assert_eq!(
        WindowTaskCommand::Restore.timeout(3),
        Duration::from_secs(240)
    );
    assert_eq!(
        WindowTaskCommand::Rehearse.timeout(0),
        Duration::from_secs(110)
    );
    assert!(WindowTaskCommand::Probe.timeout(64) > WindowTaskCommand::Probe.timeout(1));
}
