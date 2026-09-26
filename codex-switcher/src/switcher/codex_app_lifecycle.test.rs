use super::validate_stop_target_with;
use crate::distribution::WindowProcessIdentity;

#[test]
fn shutdown_requires_same_exact_process_after_capture() {
    let expected = WindowProcessIdentity::new(9999, "123:456789").unwrap();
    let changed_birth = WindowProcessIdentity::new(9999, "124:456789").unwrap();
    assert!(validate_stop_target_with(&expected, || vec![9999], |_| Ok(expected.clone())).is_ok());
    assert!(validate_stop_target_with(&expected, || vec![9999], |_| Ok(changed_birth)).is_err());
    assert!(validate_stop_target_with(
        &expected,
        || vec![10000],
        |_| { panic!("a different PID must not be inspected") }
    )
    .is_err());
}
