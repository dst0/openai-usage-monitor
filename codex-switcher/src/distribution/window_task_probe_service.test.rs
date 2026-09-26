use super::*;
use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;
use std::rc::Rc;

const BIRTH: &str = "1726789012:000007";
const TASK_LINK: &str = "codex://threads/01a00000-0000-4000-8000-00000000000f";

/// A temporary directory holding a fake window helper and a Codex home.
/// Nothing here resolves the installed helper or the live `~/.codex`.
struct ProbeFixture {
    root: PathBuf,
}

impl ProbeFixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codex-task-probe-{label}-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::create_dir_all(root.join("home")).unwrap();
        Self { root }
    }

    fn home(&self) -> PathBuf {
        self.root.join("home")
    }

    /// Writes a helper that answers `inspect-process` for PID 4242, accepts
    /// the probe only with its exact argument list, then runs `probe_body`.
    fn helper(&self, probe_body: &str) -> SystemWindowRestoreBackend {
        let helper = self.root.join("window-helper");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  inspect-process) [ \"$#\" = 3 ] && [ \"$3\" = 4242 ] || exit 2; printf '%s' '{{\"pid\":4242,\"birth_id\":\"{BIRTH}\"}}' ;;\n  probe-selected-tasks) [ \"$#\" = 7 ] && [ \"$2\" = --expected-pid ] && [ \"$3\" = 4242 ] && [ \"$4\" = --expected-birth ] && [ \"$5\" = '{BIRTH}' ] && [ \"$6\" = --allow-focus-and-clipboard ] && [ \"$7\" = yes ] || {{ printf 'ARGUMENTS_REJECTED\\n' >&2; exit 2; }}\n    {probe_body} ;;\n  *) exit 3 ;;\nesac\n"
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        SystemWindowRestoreBackend::with_helper(helper)
    }

    fn run(&self, probe_body: &str) -> Result<usize, String> {
        let backend = self.helper(probe_body);
        WindowTaskProbeService::run(
            true,
            || Ok(()),
            || self.home(),
            || Ok(vec![4242]),
            || Ok(backend),
        )
    }
}

impl Drop for ProbeFixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn success_body(extra: &str) -> String {
    format!(
        "printf '%s' '{{\"process\":{{\"pid\":4242,\"birth_id\":\"{BIRTH}\"}},\"window_ids\":[31,32],\"observed_task_count\":2{extra}}}'"
    )
}

fn unreachable_home() -> PathBuf {
    panic!("the keymap must not be read before the earlier checks pass")
}

fn unreachable_pids() -> Result<Vec<u32>, String> {
    panic!("the process table must not be read before the earlier checks pass")
}

fn unreachable_backend() -> Result<SystemWindowRestoreBackend, String> {
    panic!("the window helper must not be resolved before the earlier checks pass")
}

#[test]
fn refuses_without_opt_in_before_any_other_step() {
    let result = WindowTaskProbeService::run(
        false,
        || -> Result<(), String> { panic!("the operation lock must not be taken") },
        unreachable_home,
        unreachable_pids,
        unreachable_backend,
    );
    assert_eq!(result, Err(OPT_IN_REQUIRED.into()));
}

#[test]
fn a_running_switch_or_recovery_blocks_the_probe() {
    let result = WindowTaskProbeService::run(
        true,
        || -> Result<(), String> { Err("Another desktop switch/recovery is in progress".into()) },
        unreachable_home,
        unreachable_pids,
        unreachable_backend,
    );
    assert_eq!(
        result,
        Err("Another desktop switch/recovery is in progress".into())
    );
}

#[test]
fn a_custom_keymap_stops_the_probe_before_it_reaches_desktop() {
    let fixture = ProbeFixture::new("keymap");
    std::fs::write(
        fixture.home().join("keybindings.json"),
        r#"[{"command":"archiveThread","key":"CmdOrCtrl+Alt+L"}]"#,
    )
    .unwrap();
    let result = WindowTaskProbeService::run(
        true,
        || Ok(()),
        || fixture.home(),
        unreachable_pids,
        unreachable_backend,
    );
    assert!(result
        .unwrap_err()
        .contains("default Copy deeplink shortcut"));
}

#[test]
fn requires_exactly_one_desktop_process_before_resolving_the_helper() {
    let fixture = ProbeFixture::new("pids");
    for pids in [vec![], vec![4242, 4243]] {
        let result = WindowTaskProbeService::run(
            true,
            || Ok(()),
            || fixture.home(),
            || Ok(pids.clone()),
            unreachable_backend,
        );
        assert_eq!(
            result,
            Err("Task probe requires exactly one ChatGPT main process".into())
        );
    }
    let result = WindowTaskProbeService::run(
        true,
        || Ok(()),
        || fixture.home(),
        || Err("process table unavailable".into()),
        unreachable_backend,
    );
    assert_eq!(result, Err("process table unavailable".into()));
}

#[test]
fn holds_the_operation_lock_until_the_helper_has_answered() {
    struct Operation(Rc<Cell<bool>>);
    impl Drop for Operation {
        fn drop(&mut self) {
            self.0.set(true);
        }
    }
    let fixture = ProbeFixture::new("lock");
    let released = Rc::new(Cell::new(false));
    let backend = fixture.helper(&success_body(""));
    let result = WindowTaskProbeService::run(
        true,
        || Ok(Operation(released.clone())),
        || {
            assert!(!released.get(), "keymap read after the lock was released");
            fixture.home()
        },
        || {
            assert!(
                !released.get(),
                "process lookup after the lock was released"
            );
            Ok(vec![4242])
        },
        || {
            assert!(
                !released.get(),
                "helper resolved after the lock was released"
            );
            Ok(backend)
        },
    );
    assert_eq!(result, Ok(2));
    assert!(released.get(), "the operation lock was never released");
}

#[test]
fn passes_the_explicit_opt_in_and_exact_process_to_the_helper() {
    let fixture = ProbeFixture::new("args");
    assert_eq!(fixture.run(&success_body("")), Ok(2));
    let summary = WindowTaskProbeService::summary(2);
    assert!(summary.starts_with("Task probe: 2 ChatGPT window(s)"));
    assert!(summary.contains("No restart"));
}

#[test]
fn helper_failures_are_named_without_echoing_helper_output() {
    let fixture = ProbeFixture::new("failure");
    assert_eq!(
        fixture.run("printf 'WINDOW_FOCUS_FAILED\\n' >&2; exit 1"),
        Err("Task probe failed: WINDOW_FOCUS_FAILED".into())
    );
    let leaked = fixture
        .run(&format!("printf '{TASK_LINK}\\n' >&2; exit 1"))
        .unwrap_err();
    assert_eq!(leaked, "Codex window restore helper rejected the request");
    let extra = fixture
        .run(&success_body(&format!(",\"task_ids\":[\"{TASK_LINK}\"]")))
        .unwrap_err();
    assert!(!extra.contains("codex://"), "{extra}");
}

#[test]
fn a_changed_process_identity_is_rejected() {
    let fixture = ProbeFixture::new("identity");
    let changed = success_body("").replace(BIRTH, "1726789012:000008");
    assert_eq!(
        fixture.run(&changed),
        Err("Task probe process identity changed".into())
    );
}
