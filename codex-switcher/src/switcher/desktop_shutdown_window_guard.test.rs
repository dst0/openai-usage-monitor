use super::DesktopShutdownWindowGuard as Guard;
use crate::distribution::WindowProcessIdentity;
use std::cell::Cell;

fn identity(birth: &str) -> WindowProcessIdentity {
    WindowProcessIdentity::new(4242, birth).unwrap()
}

#[test]
fn shutdown_requires_same_exact_process_after_capture() {
    let expected = WindowProcessIdentity::new(9999, "123:456789").unwrap();
    let changed_birth = WindowProcessIdentity::new(9999, "124:456789").unwrap();
    assert!(Guard::validate(&[9999], &expected, &expected, &[7], &[9999], None).is_ok());
    assert!(Guard::validate(&[9999], &expected, &changed_birth, &[7], &[9999], None).is_err());
    assert!(Guard::validate(&[10000], &expected, &expected, &[7], &[10000], None).is_err());
}

#[test]
fn multiwindow_inventory_blocks_shutdown_before_signalling() {
    let old = identity("123:456");
    let calls = Cell::new(0);
    let two = Guard::snapshot_for_test(&[4242], &old, &[31, 32], &[4242]);
    let error = Guard::signal_after_validation(&two, &old, None, |_| {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .unwrap_err();
    assert!(error.contains("--restore-window-tasks"), "{error}");
    assert_eq!(calls.get(), 0, "multiwindow shutdown must not send SIGTERM");
    assert!(Guard::validate(&[4242], &old, &old, &[], &[4242], None).is_ok());
    assert!(Guard::validate(&[4242], &old, &old, &[31], &[4242], None).is_ok());
    let one = Guard::snapshot_for_test(&[4242], &old, &[31], &[4242]);
    Guard::signal_after_validation(&one, &old, None, |_| {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn captured_windows_allow_exactly_those_windows() {
    let old = identity("123:456");
    let calls = Cell::new(0);
    let captured = [31, 32, 33];
    let same = Guard::snapshot_for_test(&[4242], &old, &captured, &[4242]);
    Guard::signal_after_validation(&same, &old, Some(&captured), |pid| {
        assert_eq!(pid, 4242);
        calls.set(calls.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    // A window opened, closed, or replaced after the capture has no
    // captured task, so the restart stops before any signal.
    for changed in [&[31, 32][..], &[31, 32, 33, 34], &[31, 32, 34], &[]] {
        let snapshot = Guard::snapshot_for_test(&[4242], &old, changed, &[4242]);
        assert!(
            Guard::signal_after_validation(&snapshot, &old, Some(&captured), |_| {
                calls.set(calls.get() + 1);
                Ok(())
            })
            .is_err()
        );
    }
    assert_eq!(calls.get(), 1);
    // A capture never relaxes the exact-process checks.
    let recycled = identity("123:457");
    assert!(Guard::validate(
        &[4242],
        &old,
        &recycled,
        &captured,
        &[4242],
        Some(&captured)
    )
    .is_err());
    assert!(Guard::validate(&[4242], &old, &old, &captured, &[4243], Some(&captured)).is_err());
}

#[test]
fn verified_zero_window_capture_rejects_a_new_window() {
    let old = identity("123:456");
    let calls = Cell::new(0);
    let one = Guard::snapshot_for_test(&[4242], &old, &[31], &[4242]);
    assert!(Guard::signal_after_validation(&one, &old, Some(&[]), |_| {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .is_err());
    assert_eq!(calls.get(), 0);
}

#[test]
fn changed_or_multiple_desktop_processes_block_shutdown() {
    let old = identity("123:456");
    let calls = Cell::new(0);
    assert!(Guard::validate(&[4242], &old, &old, &[31], &[4243], None).is_err());
    assert!(Guard::validate(&[4242, 4243], &old, &old, &[31], &[4242, 4243], None).is_err());
    assert!(Guard::validate(&[4242], &old, &old, &[31], &[], None).is_err());
    let recycled = identity("123:457");
    assert!(Guard::validate(&[4242], &old, &recycled, &[31], &[4242], None).is_err());
    let snapshot = Guard::snapshot_for_test(&[4242], &recycled, &[31], &[4242]);
    assert!(Guard::signal_after_validation(&snapshot, &old, None, |_| {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .is_err());
    assert_eq!(calls.get(), 0, "recycled PID must not receive SIGTERM");
}
