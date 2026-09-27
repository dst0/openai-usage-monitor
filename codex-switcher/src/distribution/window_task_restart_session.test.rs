use super::*;
use std::os::unix::fs::PermissionsExt;

const BIRTH: &str = "1726789012:000007";
const RELAUNCHED_BIRTH: &str = "1726789999:000001";
const A: &str = "01a00000-0000-4000-8000-00000000000a";
const B: &str = "01a00000-0000-4000-8000-00000000000b";

/// A temporary Codex home and fake window helper; nothing here resolves the
/// installed helper, the live `~/.codex`, or a Desktop process.
struct Fixture {
    root: PathBuf,
}

impl Fixture {
    fn new(label: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "codex-window-task-restart-{label}-{}-{}",
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

    fn path(&self, name: &str) -> PathBuf {
        self.root.join(name)
    }

    /// Answers `inspect-process` for the relaunched PID 5151, records each
    /// window-task call's arguments and stdin, then runs the matching body.
    fn backend(&self, snapshot_body: &str, restore_body: &str) -> SystemWindowRestoreBackend {
        let helper = self.path("window-helper");
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  inspect-process) printf '%s' '{{\"pid\":5151,\"birth_id\":\"{RELAUNCHED_BIRTH}\"}}' ;;\n  snapshot-window-tasks) printf '%s\\n' \"$*\" > '{snapshot_args}'; {snapshot_body} ;;\n  restore-window-tasks) printf '%s\\n' \"$*\" >> '{restore_args}'; cat > '{plan}'; {restore_body} ;;\n  *) exit 3 ;;\nesac\n",
            snapshot_args = self.path("snapshot-args").display(),
            restore_args = self.path("restore-args").display(),
            plan = self.path("plan").display(),
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        SystemWindowRestoreBackend::with_helper(helper)
    }

    fn restore_calls(&self) -> usize {
        std::fs::read_to_string(self.path("restore-args"))
            .map(|text| text.lines().count())
            .unwrap_or(0)
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.root);
    }
}

fn expected() -> ProcessIdentity {
    ProcessIdentity::new(4242, BIRTH).unwrap()
}

fn snapshot_body(birth: &str) -> String {
    format!(
        "printf '%s' '{{\"process\":{{\"pid\":4242,\"birth_id\":\"{birth}\"}},\"windows\":[\
         {{\"window_id\":31,\"frame\":{{\"x\":0,\"y\":30,\"width\":900,\"height\":700}},\"task_id\":\"{A}\",\"focused\":false}},\
         {{\"window_id\":32,\"frame\":{{\"x\":950,\"y\":30,\"width\":900,\"height\":700}},\"task_id\":\"{B}\",\"focused\":true}}],\
         \"clipboard_restored\":true}}'"
    )
}

fn restore_body(verified: &str) -> String {
    format!(
        "printf '%s' '{{\"process\":{{\"pid\":5151,\"birth_id\":\"{RELAUNCHED_BIRTH}\"}},\"window_ids\":[41,42],\"verified\":{verified},\"clipboard_restored\":true}}'"
    )
}

fn capture(
    fixture: &Fixture,
    backend: &SystemWindowRestoreBackend,
) -> Result<WindowTaskRestartSession, String> {
    WindowTaskRestartSession::capture(&expected(), Ok(fixture.home()), backend, &[31, 32])
}

#[test]
fn capture_reads_every_window_of_the_exact_process() {
    let fixture = Fixture::new("capture");
    let backend = fixture.backend(&snapshot_body(BIRTH), "exit 9");
    let session = capture(&fixture, &backend).unwrap();
    assert_eq!(session.window_ids(), vec![31, 32]);
    assert_eq!(session.window_count(), 2);
    assert_eq!(
        std::fs::read_to_string(fixture.path("snapshot-args")).unwrap(),
        format!("snapshot-window-tasks --expected-pid 4242 --expected-birth {BIRTH} --allow-focus-and-clipboard yes\n")
    );
}

#[test]
fn capture_refuses_before_the_helper_without_desktop_home_or_copy_binding() {
    let fixture = Fixture::new("capture-refuse");
    let backend = fixture.backend(&snapshot_body(BIRTH), "exit 9");
    assert_eq!(
        WindowTaskRestartSession::capture(
            &expected(),
            Err("not the Desktop home".into()),
            &backend,
            &[31, 32]
        )
        .err(),
        Some("not the Desktop home".into())
    );
    std::fs::write(
        fixture.home().join("keybindings.json"),
        r#"[{"command":"copyDeeplink","key":null}]"#,
    )
    .unwrap();
    assert!(capture(&fixture, &backend).is_err());
    assert!(
        !fixture.path("snapshot-args").exists(),
        "the helper must not run"
    );
}

#[test]
fn capture_fails_closed_on_any_mismatch_or_helper_failure() {
    let fixture = Fixture::new("capture-fail");
    let backend = fixture.backend(&snapshot_body(BIRTH), "exit 9");
    assert_eq!(
        WindowTaskRestartSession::capture(&expected(), Ok(fixture.home()), &backend, &[31]).err(),
        Some(WINDOWS_CHANGED.into())
    );
    let recycled = fixture.backend(&snapshot_body("1726789012:000008"), "exit 9");
    let error = capture(&fixture, &recycled).err().unwrap();
    assert!(
        error.starts_with("Window task helper process identity changed; "),
        "{error}"
    );
    let failed = fixture.backend(
        "printf 'COPY_LINK_MISSING after-focus\\n' >&2; exit 1",
        "exit 9",
    );
    assert_eq!(
        capture(&fixture, &failed).err(),
        Some(
            "Window task snapshot failed: COPY_LINK_MISSING after it began focusing ChatGPT \
             windows; the clipboard may now hold a copied task link"
                .into()
        )
    );
    let keymap = fixture.home().join("keybindings.json");
    let edit = format!(
        "printf '%s' '[{{\"command\":\"archiveThread\",\"key\":\"CmdOrCtrl+Alt+L\"}}]' > '{}'; ",
        keymap.display()
    );
    let edited = fixture.backend(&(edit + &snapshot_body(BIRTH)), "exit 9");
    assert_eq!(
        capture(&fixture, &edited).err(),
        Some(KEYMAP_CHANGED.into())
    );
}

#[test]
fn restore_sends_the_plan_to_the_relaunched_process_through_stdin() {
    let fixture = Fixture::new("restore");
    let mut backend = fixture.backend(&snapshot_body(BIRTH), &restore_body("[true,true]"));
    let mut session = capture(&fixture, &backend).unwrap();
    session.restore(5151, &mut backend, || Ok(()), "after relaunch");
    assert_eq!(
        std::fs::read_to_string(fixture.path("restore-args")).unwrap(),
        format!("restore-window-tasks --expected-pid 5151 --expected-birth {RELAUNCHED_BIRTH} --allow-focus-and-clipboard yes\n")
    );
    let plan: serde_json::Value =
        serde_json::from_slice(&std::fs::read(fixture.path("plan")).unwrap()).unwrap();
    assert_eq!(plan["windows"][0]["task_id"], A);
    assert_eq!(plan["windows"][1]["task_id"], B);
    assert_eq!(plan["windows"][1]["frame"]["x"], 950.0);
    assert_eq!(plan["focus_index"], 1);
    assert_eq!(session.finish(), Ok(()));
}

#[test]
fn restore_failures_are_recorded_without_task_ids() {
    let fixture = Fixture::new("restore-fail");
    let mut backend = fixture.backend(&snapshot_body(BIRTH), &restore_body("[true,false]"));
    let mut session = capture(&fixture, &backend).unwrap();
    session.restore(5151, &mut backend, || Ok(()), "after relaunch");
    session.restore(
        5151,
        &mut backend,
        || Err("Desktop IPC is not ready".into()),
        "after recovery",
    );
    assert_eq!(
        fixture.restore_calls(),
        1,
        "an unready Desktop gets no task link"
    );
    session.record_failure(
        "after recovery",
        "Codex window restore helper is not installed",
    );
    let error = session.finish().unwrap_err();
    assert!(
        error.contains("after relaunch: 1 of 2 window(s) verified on their task"),
        "{error}"
    );
    assert!(
        error.contains("after recovery: Desktop IPC is not ready"),
        "{error}"
    );
    assert!(error.contains("helper is not installed"), "{error}");
    assert!(!error.contains(A) && !error.contains(B), "{error}");
}

#[test]
fn a_keymap_changed_before_restore_stops_the_shortcut() {
    let fixture = Fixture::new("restore-keymap");
    let mut backend = fixture.backend(&snapshot_body(BIRTH), &restore_body("[true,true]"));
    let mut session = capture(&fixture, &backend).unwrap();
    std::fs::write(fixture.home().join("keybindings.json"), "[]").unwrap();
    session.restore(5151, &mut backend, || Ok(()), "after relaunch");
    assert_eq!(fixture.restore_calls(), 0);
    assert!(session.finish().unwrap_err().contains(KEYMAP_CHANGED));
}

#[test]
fn a_named_restore_failure_is_reported_for_its_phase() {
    let fixture = Fixture::new("restore-named");
    let mut backend = fixture.backend(
        &snapshot_body(BIRTH),
        "printf 'NEW_WINDOW_UNAVAILABLE after-focus\\n' >&2; exit 1",
    );
    let mut session = capture(&fixture, &backend).unwrap();
    session.restore(5151, &mut backend, || Ok(()), "after relaunch");
    let error = session.finish().unwrap_err();
    assert!(
        error.contains("after relaunch: Window task restore failed: NEW_WINDOW_UNAVAILABLE"),
        "{error}"
    );
}
