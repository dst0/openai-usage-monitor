use super::WindowProcessValidationService;
use crate::distribution::window_restore_backend::WindowRestoreBackend;
use crate::distribution::window_restore_capture::WindowCapture;
use crate::distribution::window_restore_process_identity::ProcessIdentity;

struct InspectOnlyBackend {
    result: Result<ProcessIdentity, String>,
    calls: usize,
}

impl WindowRestoreBackend for InspectOnlyBackend {
    fn inspect_process(&mut self, _expected_pid: u32) -> Result<ProcessIdentity, String> {
        self.calls += 1;
        self.result.clone()
    }

    fn capture_main_window(&mut self, _: ProcessIdentity) -> Result<WindowCapture, String> {
        panic!("Accessibility must not be queried when preservation is disabled")
    }

    fn set_position(&mut self, _: ProcessIdentity, _: (f64, f64)) -> Result<(), String> {
        panic!("Window position must not be changed")
    }

    fn set_size(&mut self, _: ProcessIdentity, _: (f64, f64)) -> Result<(), String> {
        panic!("Window size must not be changed")
    }

    fn read_main_window(&mut self, _: ProcessIdentity) -> Result<WindowCapture, String> {
        panic!("Accessibility must not be queried after restart")
    }
}

#[test]
fn valid_exact_process_requires_only_inspection() {
    let expected = ProcessIdentity::new(77, "100:000001").unwrap();
    let mut backend = InspectOnlyBackend {
        result: Ok(expected.clone()),
        calls: 0,
    };
    assert_eq!(
        WindowProcessValidationService::inspect(&mut backend, 77).unwrap(),
        expected
    );
    assert_eq!(backend.calls, 1);
}

#[test]
fn helper_failure_or_mismatched_pid_blocks_windowless_recovery() {
    let cases = [
        Err("PROCESS_IDENTITY_REJECTED".to_string()),
        Ok(ProcessIdentity::new(78, "100:000001").unwrap()),
        Ok(ProcessIdentity {
            pid: 77,
            birth_id: String::new(),
        }),
    ];
    for result in cases {
        let mut backend = InspectOnlyBackend { result, calls: 0 };
        assert!(WindowProcessValidationService::inspect(&mut backend, 77).is_err());
        assert_eq!(backend.calls, 1);
    }
}

#[test]
fn invalid_expected_pid_never_calls_helper() {
    let mut backend = InspectOnlyBackend {
        result: Ok(ProcessIdentity::new(77, "100:000001").unwrap()),
        calls: 0,
    };
    assert!(WindowProcessValidationService::inspect(&mut backend, 0).is_err());
    assert_eq!(backend.calls, 0);
}

#[test]
fn process_birth_change_before_shutdown_is_rejected() {
    let expected = ProcessIdentity::new(77, "100:000001").unwrap();
    let mut backend = InspectOnlyBackend {
        result: Ok(ProcessIdentity::new(77, "101:000001").unwrap()),
        calls: 0,
    };
    assert!(WindowProcessValidationService::confirm(&mut backend, &expected).is_err());
    assert_eq!(backend.calls, 1);
}
