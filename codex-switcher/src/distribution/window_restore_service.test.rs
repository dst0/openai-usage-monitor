use crate::distribution::{
    window_restore_backend::WindowRestoreBackend, window_restore_capture::WindowCapture,
    window_restore_frame::WindowFrame, window_restore_outcome::RestoreOutcome,
    window_restore_process_identity::ProcessIdentity, window_restore_sanitizer::sanitize_text,
    window_restore_screen::ScreenIdentity, window_restore_service::WindowRestoreService,
    window_restore_tolerance::RestoreTolerance,
};

struct MockBackend {
    identity: ProcessIdentity,
    captured: WindowCapture,
    working: WindowCapture,
    final_read: Option<WindowCapture>,
    calls: Vec<&'static str>,
    positions: Vec<(f64, f64)>,
    failure: Option<&'static str>,
}

impl MockBackend {
    fn new() -> Self {
        let identity = ProcessIdentity::new(77, "9001:000001").unwrap();
        let frame = WindowFrame {
            x: -1440.0,
            y: 90.0,
            width: 920.0,
            height: 680.0,
        };
        let capture = WindowCapture {
            process: identity.clone(),
            frame,
            screen: ScreenIdentity {
                display_id: 2,
                frame: WindowFrame {
                    x: -1920.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                },
            },
        };
        Self {
            identity,
            captured: capture.clone(),
            working: capture.clone(),
            final_read: None,
            calls: Vec::new(),
            positions: Vec::new(),
            failure: None,
        }
    }

    fn fails_at(mut self, phase: &'static str) -> Self {
        self.failure = Some(phase);
        self
    }
}

impl WindowRestoreBackend for MockBackend {
    fn inspect_process(&mut self, _expected_pid: u32) -> Result<ProcessIdentity, String> {
        self.calls.push("inspect");
        if self.failure == Some("inspect") {
            Err("helper /Users/dst/private\nemail@example.com".into())
        } else {
            Ok(self.identity.clone())
        }
    }

    fn capture_main_window(&mut self, _process: ProcessIdentity) -> Result<WindowCapture, String> {
        self.calls.push("capture");
        if self.failure == Some("capture") {
            Err("AX query failed for /Users/dst/window".into())
        } else {
            Ok(self.captured.clone())
        }
    }

    fn set_position(
        &mut self,
        _process: ProcessIdentity,
        position: (f64, f64),
    ) -> Result<(), String> {
        self.calls.push("position");
        self.positions.push(position);
        let occurrence = self.positions.len();
        if self.failure == Some("position-initial") && occurrence == 1 {
            return Err("AppleScript failed at /private/window".into());
        }
        if self.failure == Some("position-final") && occurrence == 2 {
            return Err("AppleScript final write failed".into());
        }
        self.working.frame.x = position.0;
        self.working.frame.y = position.1;
        Ok(())
    }

    fn set_size(&mut self, _process: ProcessIdentity, size: (f64, f64)) -> Result<(), String> {
        self.calls.push("size");
        if self.failure == Some("size") {
            return Err("helper failed for /Users/dst/window".into());
        }
        self.working.frame.width = size.0;
        self.working.frame.height = size.1;
        Ok(())
    }

    fn read_main_window(&mut self, _process: ProcessIdentity) -> Result<WindowCapture, String> {
        self.calls.push("read");
        if self.failure == Some("read") {
            return Err("read failed for /Users/dst/window".into());
        }
        Ok(self
            .final_read
            .clone()
            .unwrap_or_else(|| self.working.clone()))
    }
}

fn service() -> WindowRestoreService {
    WindowRestoreService::new(RestoreTolerance::new(1.0, 1.0).unwrap()).unwrap()
}

#[test]
fn captures_exact_process_window_and_secondary_negative_origin() {
    let mut backend = MockBackend::new();
    let result = service().capture(&mut backend, "op-1", "restart", 77);
    let capture = result.capture.expect("capture should succeed");
    assert_eq!(capture.process, backend.identity);
    assert_eq!(capture.screen.display_id, 2);
    assert_eq!(capture.screen.frame.x, -1920.0);
    assert_eq!(capture.frame.x, -1440.0);
    assert_eq!(result.report.outcome, RestoreOutcome::Restored);
}

#[test]
fn restore_requires_exact_pid_and_birth_identity() {
    let mut backend = MockBackend::new();
    let capture = backend.captured.clone();
    backend.identity.birth_id = "9001:000002".into();
    let report = service().restore(&mut backend, "op-2", "switch", capture);
    assert_eq!(report.outcome, RestoreOutcome::Failed);
    assert_eq!(backend.calls, vec!["inspect"]);
    assert!(report
        .events
        .last()
        .unwrap()
        .detail
        .contains("birth identity"));
}

#[test]
fn restore_uses_position_size_position_and_accepts_tolerance() {
    let mut backend = MockBackend::new();
    let mut actual = backend.captured.clone();
    actual.frame.x += 0.5;
    actual.frame.width -= 0.5;
    backend.final_read = Some(actual);
    let capture = backend.captured.clone();
    let report = service().restore(&mut backend, "op-3", "restart", capture);
    assert_eq!(report.outcome, RestoreOutcome::Restored);
    assert_eq!(
        backend.calls,
        vec!["inspect", "position", "size", "position", "read"]
    );
    assert_eq!(backend.positions, vec![(-1440.0, 90.0), (-1440.0, 90.0)]);
}

#[test]
fn restore_reports_partial_when_post_restore_bounds_mismatch() {
    let mut backend = MockBackend::new();
    let mut actual = backend.captured.clone();
    actual.frame.x += 4.0;
    backend.final_read = Some(actual);
    let capture = backend.captured.clone();
    let report = service().restore(&mut backend, "op-4", "restart", capture);
    assert_eq!(report.outcome, RestoreOutcome::Partial);
    assert!(report
        .events
        .iter()
        .any(|event| event.phase == "RESTORE_VERIFY_BOUNDS"));
}

#[test]
fn helper_failure_after_position_is_partial_and_propagated() {
    let mut backend = MockBackend::new().fails_at("size");
    let capture = backend.captured.clone();
    let report = service().restore(&mut backend, "op-5", "quota", capture);
    assert_eq!(report.outcome, RestoreOutcome::Partial);
    assert_eq!(backend.calls, vec!["inspect", "position", "size"]);
    let detail = &report.events.last().unwrap().detail;
    assert!(!detail.contains("/Users"));
    assert!(!detail.contains('@'));
}

#[test]
fn failed_capture_does_not_return_a_successful_restore_target() {
    let mut backend = MockBackend::new().fails_at("capture");
    let result = service().capture(&mut backend, "op-6", "restart", 77);
    assert!(result.capture.is_none());
    assert_eq!(result.report.outcome, RestoreOutcome::Failed);
    assert!(result
        .report
        .events
        .last()
        .unwrap()
        .detail
        .contains("[redacted-path]"));
}

#[test]
fn audit_details_redact_uuid_fragments_inside_key_value_text() {
    let sanitized = sanitize_text("turn=123e4567-e89b-12d3-a456-426614174000");
    assert_eq!(sanitized, "[redacted-id]");
}
