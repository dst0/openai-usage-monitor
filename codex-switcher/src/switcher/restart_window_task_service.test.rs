use super::RestartWindowTaskService;
use crate::distribution::{
    SystemWindowRestoreBackend, WindowProcessIdentity, WindowTaskRestartSession,
};
use std::cell::RefCell;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

const BIRTH: &str = "1726789012:000007";
const A: &str = "01a00000-0000-4000-8000-00000000000a";
const B: &str = "01a00000-0000-4000-8000-00000000000b";

/// A temporary Codex home and fake window helper that records each restore
/// plan; nothing here resolves the installed helper or a live Desktop.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codex-restart-window-task-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("home")).unwrap();
        let helper = root.join("window-helper");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  snapshot-window-tasks) printf '%s' '{{\"process\":{{\"pid\":4242,\"birth_id\":\"{BIRTH}\"}},\"windows\":[{{\"window_id\":31,\"frame\":{{\"x\":0,\"y\":30,\"width\":900,\"height\":700}},\"task_id\":\"{A}\",\"focused\":true}},{{\"window_id\":32,\"frame\":{{\"x\":950,\"y\":30,\"width\":900,\"height\":700}},\"task_id\":\"{B}\",\"focused\":false}}],\"clipboard_restored\":true}}' ;;\n  restore-window-tasks) cat >> '{plans}'; printf '\\n' >> '{plans}'; printf '%s' '{{\"process\":{{\"pid\":4242,\"birth_id\":\"{BIRTH}\"}},\"verified\":[true,true],\"clipboard_restored\":true}}' ;;\n  *) exit 3 ;;\nesac\n",
            plans = root.join("plans").display()
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { root }
    }

    fn backend(&self) -> SystemWindowRestoreBackend {
        SystemWindowRestoreBackend::with_helper(self.root.join("window-helper"))
    }

    fn session(&self) -> Option<WindowTaskRestartSession> {
        Some(
            WindowTaskRestartSession::capture(
                &process(),
                Ok(self.root.join("home")),
                &self.backend(),
                &[31, 32],
            )
            .unwrap(),
        )
    }

    /// The `mode` of each restore plan the helper received, in order.
    fn modes(&self) -> Vec<String> {
        std::fs::read_to_string(self.root.join("plans"))
            .unwrap_or_default()
            .lines()
            .map(|plan| {
                serde_json::from_str::<serde_json::Value>(plan).unwrap()["mode"]
                    .as_str()
                    .unwrap()
                    .to_string()
            })
            .collect()
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn process() -> WindowProcessIdentity {
    WindowProcessIdentity::new(4242, BIRTH).unwrap()
}

/// Without the request nothing is resolved: no helper, no Codex home, no
/// window inventory. Test builds panic on any of those, so reaching this
/// assertion proves the unrequested path stays inert.
#[test]
fn an_unrequested_restart_touches_no_window() {
    let mut session = RestartWindowTaskService::capture_if_requested(false, &process()).unwrap();
    assert!(session.is_none());
    assert_eq!(RestartWindowTaskService::captured_windows(&session), None);
    let targets = [A.to_string()];
    let recovered =
        RestartWindowTaskService::around_recovery(&mut session, &process(), &targets, || Ok(()));
    assert_eq!(recovered, Ok(()));
    assert_eq!(RestartWindowTaskService::finish(session), Ok(()));
}

#[test]
fn windows_are_rechecked_after_recovery_even_when_recovery_fails() {
    let fixture = Fixture::new("recovery-failed");
    let mut session = fixture.session();
    let order = RefCell::new(Vec::new());
    let targets = [A.to_string()];
    let result = RestartWindowTaskService::around_recovery_with(
        &mut session,
        &process(),
        &targets,
        || {
            order.borrow_mut().push("backend");
            Ok(fixture.backend())
        },
        || Ok(()),
        || {
            order.borrow_mut().push("recover");
            Err("partial recovery".into())
        },
    );
    assert_eq!(result, Err("partial recovery".into()));
    assert_eq!(*order.borrow(), ["backend", "recover", "backend"]);
    assert_eq!(fixture.modes(), ["relaunch", "recheck"]);
    assert_eq!(RestartWindowTaskService::finish(session), Ok(()));
}

#[test]
fn without_recovery_targets_only_the_relaunch_restore_runs() {
    let fixture = Fixture::new("no-targets");
    let mut session = fixture.session();
    let result = RestartWindowTaskService::around_recovery_with(
        &mut session,
        &process(),
        &[],
        || Ok(fixture.backend()),
        || Ok(()),
        || Ok(()),
    );
    assert_eq!(result, Ok(()));
    assert_eq!(fixture.modes(), ["relaunch"]);
}

#[test]
fn a_missing_helper_is_reported_after_the_restart() {
    let fixture = Fixture::new("no-helper");
    let mut session = fixture.session();
    let result = RestartWindowTaskService::around_recovery_with(
        &mut session,
        &process(),
        &[],
        || Err("Codex window restore helper is not installed".into()),
        || Ok(()),
        || Ok(()),
    );
    assert_eq!(
        result,
        Ok(()),
        "a window failure never becomes a recovery failure"
    );
    let error = RestartWindowTaskService::finish(session).unwrap_err();
    assert!(
        error.contains("after relaunch: Codex window restore helper is not installed"),
        "{error}"
    );
}

#[test]
fn window_failures_are_added_to_other_restart_failures() {
    assert_eq!(RestartWindowTaskService::append_failure(None, Ok(())), None);
    assert_eq!(
        RestartWindowTaskService::append_failure(Some("recovery failed".into()), Ok(())),
        Some("recovery failed".into())
    );
    assert_eq!(
        RestartWindowTaskService::append_failure(None, Err("windows".into())),
        Some("windows".into())
    );
    assert_eq!(
        RestartWindowTaskService::append_failure(
            Some("recovery failed".into()),
            Err("windows".into())
        ),
        Some("recovery failed; windows".into())
    );
}
