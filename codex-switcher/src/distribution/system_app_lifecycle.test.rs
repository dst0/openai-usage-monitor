use super::*;

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
