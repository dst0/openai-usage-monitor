use super::*;
use crate::distribution::window_restore_report::RestoreReport;

fn failed_capture(phase: &str, detail: &str) -> RestoreReport {
    let mut report = RestoreReport::new("op_test", "quota_exhausted");
    report.outcome = RestoreOutcome::Failed;
    report.record(phase, RestoreOutcome::Failed, detail);
    report
}

#[test]
fn only_explicit_missing_window_allows_windowless_switch() {
    let absent = failed_capture(
        "CAPTURE_WINDOW_FAILED",
        "Main window capture failed: WINDOW_NOT_FOUND",
    );
    assert_eq!(
        classify_capture_failure(&absent),
        Ok(WindowCaptureMode::Absent)
    );
    for (phase, detail) in [
        (
            "CAPTURE_WINDOW_FAILED",
            "Main window capture failed: WINDOW_ACCESS_FAILED",
        ),
        (
            "CAPTURE_WINDOW_FAILED",
            "Main window capture failed: WINDOW_GEOMETRY_FAILED",
        ),
        (
            "CAPTURE_WINDOW_FAILED",
            "Main window capture failed: malformed output",
        ),
        ("CAPTURE_PROCESS_MISMATCH", "Process identity changed"),
    ] {
        assert!(classify_capture_failure(&failed_capture(phase, detail)).is_err());
    }
}

#[test]
fn optional_banner_failures_exclude_identity_and_protocol_errors() {
    for error in [
        "WINDOW_NOT_FOUND",
        "WINDOW_ACCESS_FAILED",
        "WINDOW_GEOMETRY_FAILED",
    ] {
        assert!(optional_banner_capture_failure(error));
    }
    for error in [
        "PROCESS_IDENTITY_REJECTED",
        "Codex window restore helper returned invalid data",
        "Codex window restore helper could not start",
        "unexpected helper error",
    ] {
        assert!(!optional_banner_capture_failure(error));
    }
}

#[test]
fn lifecycle_capture_uses_banner_pid_birth_and_real_snapshot_parser() {
    use std::os::unix::fs::PermissionsExt;
    let root = std::env::temp_dir().join(format!(
        "system-app-task-capture-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(root.join("home")).unwrap();
    let helper = root.join("helper");
    let args = root.join("snapshot-args");
    let birth = "1726789012:000007";
    let task = "01a00000-0000-4000-8000-00000000000a";
    let script = format!(
        "#!/bin/sh\ncase \"$1\" in\n count-standard-windows) printf '%s' '{{\"process\":{{\"pid\":4242,\"birth_id\":\"{birth}\"}},\"window_ids\":[31],\"ambiguous_count\":0,\"ax_standard_count\":1}}' ;;\n snapshot-window-tasks) printf '%s\\n' \"$*\" > '{args}'; printf '%s' '{{\"process\":{{\"pid\":4242,\"birth_id\":\"{birth}\"}},\"windows\":[{{\"window_id\":31,\"frame\":{{\"x\":0,\"y\":30,\"width\":900,\"height\":700}},\"task_id\":\"{task}\",\"focused\":true}}],\"clipboard_restored\":true}}' ;;\n *) exit 2 ;;\nesac\n",
        args = args.display()
    );
    std::fs::write(&helper, script).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let expected =
        super::super::window_restore_process_identity::ProcessIdentity::new(4242, birth).unwrap();
    let lifecycle = SystemAppLifecycle::default();
    *lifecycle.recovery_banner.lock().unwrap() = Some(RecoveryBanner::without_window(expected));
    lifecycle
        .capture_window_tasks_with(
            &mut SystemWindowRestoreBackend::with_helper(helper),
            Ok(root.join("home")),
        )
        .unwrap();
    assert_eq!(
        lifecycle.window_tasks.captured_window_ids().unwrap(),
        Some(vec![31])
    );
    assert!(std::fs::read_to_string(args)
        .unwrap()
        .contains("--expected-pid 4242 --expected-birth 1726789012:000007"));
    std::fs::remove_dir_all(root).unwrap();
}
