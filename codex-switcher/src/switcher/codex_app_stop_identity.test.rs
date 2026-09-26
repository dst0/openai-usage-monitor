use super::validate_shutdown_snapshot;
use crate::distribution::WindowProcessIdentity;

#[test]
fn shutdown_requires_same_exact_process_after_capture() {
    let expected = WindowProcessIdentity::new(9999, "123:456789").unwrap();
    let changed_birth = WindowProcessIdentity::new(9999, "124:456789").unwrap();
    assert!(validate_shutdown_snapshot(&[9999], &expected, &expected, 1, &[9999]).is_ok());
    assert!(validate_shutdown_snapshot(&[9999], &expected, &changed_birth, 1, &[9999]).is_err());
    assert!(validate_shutdown_snapshot(&[10000], &expected, &expected, 1, &[10000]).is_err());
}
