use super::*;
use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::rc::Rc;

const BIRTH: &str = "1726789012:000007";
const TASK_LINK: &str = "codex://threads/01a00000-0000-4000-8000-00000000000f";
const CUSTOM_BINDING: &str = r#"[{"command":"archiveThread","key":"CmdOrCtrl+Alt+L"}]"#;

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

    /// Marker the fake helper creates when it runs the probe command.
    fn probed(&self) -> PathBuf {
        self.root.join("probed")
    }

    /// Writes a helper that answers `inspect-process` for PID 4242, accepts
    /// the probe only with its exact argument list, records that it ran,
    /// then runs `probe_body`.
    fn helper(&self, probe_body: &str) -> SystemWindowRestoreBackend {
        let helper = self.root.join("window-helper");
        let probed = self.probed();
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  inspect-process) [ \"$#\" = 3 ] && [ \"$2\" = --expected-pid ] && [ \"$3\" = 4242 ] || exit 2; printf '%s' '{{\"pid\":4242,\"birth_id\":\"{BIRTH}\"}}' ;;\n  probe-selected-tasks) [ \"$#\" = 7 ] && [ \"$2\" = --expected-pid ] && [ \"$3\" = 4242 ] && [ \"$4\" = --expected-birth ] && [ \"$5\" = '{BIRTH}' ] && [ \"$6\" = --allow-focus-and-clipboard ] && [ \"$7\" = yes ] || {{ printf 'ARGUMENTS_REJECTED\\n' >&2; exit 2; }}\n    : > '{}'\n    {probe_body} ;;\n  *) exit 3 ;;\nesac\n",
            probed.display()
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        SystemWindowRestoreBackend::with_helper(helper)
    }

    fn run(&self, probe_body: &str) -> Result<usize, String> {
        let backend = self.helper(probe_body);
        WindowTaskProbeService::run(
            true,
            || Ok(self.home()),
            || Ok(()),
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

fn unreachable_home() -> Result<PathBuf, String> {
    panic!("the Codex home must not be resolved before the opt-in")
}

fn unreachable_lock() -> Result<(), String> {
    panic!("the operation lock must not be taken before the earlier checks pass")
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
        unreachable_home,
        unreachable_lock,
        unreachable_pids,
        unreachable_backend,
    );
    assert_eq!(result, Err(OPT_IN_REQUIRED.into()));
}

#[test]
fn a_foreign_codex_home_stops_the_probe_before_the_lock() {
    let result = WindowTaskProbeService::run(
        true,
        || Err(NOT_DESKTOP_HOME.into()),
        unreachable_lock,
        unreachable_pids,
        unreachable_backend,
    );
    assert_eq!(result, Err(NOT_DESKTOP_HOME.into()));
}

#[test]
fn only_the_codex_home_chatgpt_uses_is_accepted() {
    let fixture = ProbeFixture::new("home");
    let user = fixture.root.clone();
    let desktop = user.join(".codex");
    std::fs::create_dir(&desktop).unwrap();
    assert_eq!(
        WindowTaskProbeService::desktop_codex_home(desktop.clone(), Some(user.clone())),
        Ok(desktop.clone())
    );
    // A link to the same directory is the same home.
    let alias = fixture.root.join("alias");
    std::os::unix::fs::symlink(&desktop, &alias).unwrap();
    assert_eq!(
        WindowTaskProbeService::desktop_codex_home(alias.clone(), Some(user.clone())),
        Ok(alias)
    );
    for (configured, user_home) in [
        (fixture.home(), Some(user.clone())),
        (desktop.clone(), None),
        (user.join("missing"), Some(user.clone())),
    ] {
        assert_eq!(
            WindowTaskProbeService::desktop_codex_home(configured, user_home),
            Err(NOT_DESKTOP_HOME.into())
        );
    }
    // Before ChatGPT first creates it, the default path itself still matches.
    let fresh = fixture.root.join("fresh-user");
    assert_eq!(
        WindowTaskProbeService::desktop_codex_home(fresh.join(".codex"), Some(fresh.clone())),
        Ok(fresh.join(".codex"))
    );
}

#[test]
fn a_running_switch_or_recovery_blocks_the_probe() {
    let fixture = ProbeFixture::new("busy");
    let result = WindowTaskProbeService::run(
        true,
        || Ok(fixture.home()),
        || -> Result<(), String> { Err("Another desktop switch/recovery is in progress".into()) },
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
    std::fs::write(fixture.home().join("keybindings.json"), CUSTOM_BINDING).unwrap();
    let result = WindowTaskProbeService::run(
        true,
        || Ok(fixture.home()),
        || Ok(()),
        unreachable_pids,
        unreachable_backend,
    );
    assert!(result
        .unwrap_err()
        .contains("default Copy deeplink shortcut"));
}

#[test]
fn a_keymap_edited_during_the_probe_voids_the_result() {
    let fixture = ProbeFixture::new("keymap-edit");
    let keymap = fixture.home().join("keybindings.json");
    let edit = format!("printf '%s' '{CUSTOM_BINDING}' > '{}'; ", keymap.display());
    assert_eq!(
        fixture.run(&(edit.clone() + &success_body(""))),
        Err(KEYMAP_CHANGED.into())
    );
    // Also after a helper failure, which alone would hide the edit.
    std::fs::remove_file(&keymap).unwrap();
    assert_eq!(
        fixture.run(&(edit + "printf 'WINDOW_FOCUS_FAILED after-focus\\n' >&2; exit 1")),
        Err(KEYMAP_CHANGED.into())
    );
    // A default keymap rewritten with a new modification time is an edit too.
    std::fs::write(&keymap, "[]").unwrap();
    let touch = format!("touch -m -t 203001010000 '{}'; ", keymap.display());
    assert_eq!(
        fixture.run(&(touch + &success_body(""))),
        Err(KEYMAP_CHANGED.into())
    );
}

#[test]
fn requires_exactly_one_desktop_process_before_resolving_the_helper() {
    let fixture = ProbeFixture::new("pids");
    for pids in [vec![], vec![4242, 4243]] {
        let result = WindowTaskProbeService::run(
            true,
            || Ok(fixture.home()),
            || Ok(()),
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
        || Ok(fixture.home()),
        || Ok(()),
        || Err("process table unavailable".into()),
        unreachable_backend,
    );
    assert_eq!(result, Err("process table unavailable".into()));
}

#[test]
fn holds_the_operation_lock_until_the_helper_has_answered() {
    struct Operation {
        released: Rc<Cell<bool>>,
        probed: PathBuf,
    }
    impl Drop for Operation {
        fn drop(&mut self) {
            assert!(
                Path::new(&self.probed).exists(),
                "the operation lock was released before the helper ran the probe"
            );
            self.released.set(true);
        }
    }
    let fixture = ProbeFixture::new("lock");
    let released = Rc::new(Cell::new(false));
    let backend = fixture.helper(&success_body(""));
    let result = WindowTaskProbeService::run(
        true,
        || Ok(fixture.home()),
        || {
            Ok(Operation {
                released: released.clone(),
                probed: fixture.probed(),
            })
        },
        || Ok(vec![4242]),
        || Ok(backend),
    );
    assert_eq!(result, Ok(2));
    assert!(released.get(), "the operation lock was never released");
}

#[test]
fn passes_the_explicit_opt_in_and_exact_process_to_the_helper() {
    let fixture = ProbeFixture::new("args");
    assert_eq!(fixture.run(&success_body("")), Ok(2));
    assert!(fixture.probed().exists());
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
    assert!(fixture
        .run("printf 'COPY_LINK_AMBIGUOUS after-focus\\n' >&2; exit 1")
        .unwrap_err()
        .ends_with("the clipboard may now hold a copied task link"));
    assert_eq!(
        fixture.run(&format!("printf '{TASK_LINK}\\n' >&2; exit 1")),
        Err("Codex window restore helper rejected the request".into())
    );
    assert_eq!(
        fixture.run(&success_body(&format!(",\"task_ids\":[\"{TASK_LINK}\"]"))),
        Err("Task probe returned unexpected fields".into())
    );
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
