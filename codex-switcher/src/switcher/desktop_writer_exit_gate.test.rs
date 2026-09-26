use super::*;

fn identity(pid: u32, birth: &str) -> WindowProcessIdentity {
    WindowProcessIdentity::new(pid, birth).unwrap()
}

#[test]
fn tracks_only_bundled_app_server_child_and_its_exact_birth() {
    let rows = "1 0 /sbin/launchd\n\
                100 1 /Applications/ChatGPT.app/Contents/MacOS/ChatGPT\n\
                200 100 /Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex\n\
                300 1 /Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex\n\
                400 100 /Applications/ChatGPT.app/Contents/Frameworks/Codex (Renderer).app/Contents/MacOS/Codex (Renderer)\n";
    let gate = DesktopWriterExitGate::capture_with(rows, 100, |pid| Ok(identity(pid, "11:000001")))
        .unwrap();
    assert_eq!(gate.writers.len(), 1);

    let orphaned = rows.replace("200 100 ", "200 1 ");
    assert!(gate
        .writers_running_with(&orphaned, |pid| Ok(identity(pid, "11:000001")))
        .unwrap());
    assert!(!gate
        .writers_running_with(&orphaned, |pid| Ok(identity(pid, "12:000001")))
        .unwrap());
    let exited = orphaned
        .lines()
        .filter(|line| !line.starts_with("200 "))
        .collect::<Vec<_>>()
        .join("\n");
    assert!(!gate
        .writers_running_with(&exited, |_| panic!("exited writer must not be inspected"))
        .unwrap());
}

#[test]
fn writer_probe_failure_cannot_be_mistaken_for_exit() {
    let rows = "200 100 /Applications/ChatGPT.app/Contents/Resources/codex-cli/CodexCLI.app/Contents/MacOS/codex";
    let gate = DesktopWriterExitGate::capture_with(rows, 100, |pid| Ok(identity(pid, "11:000001")))
        .unwrap();
    assert!(gate
        .writers_running_with(rows, |_| Err("inspect failed".into()))
        .is_err());
    assert!(gate
        .writers_running_with("malformed row", |_| Ok(identity(200, "11:000001")))
        .is_err());
}

#[test]
fn system_pid_rows_are_allowed_but_cannot_claim_a_desktop_writer() {
    let rows = "0 100 /Applications/ChatGPT.app/Contents/Resources/codex-cli/bin/codex\n";
    assert!(DesktopWriterExitGate::capture_with(rows, 100, |_| {
        panic!("system PID must be rejected before process inspection")
    })
    .is_err());
}
