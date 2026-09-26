use super::*;

#[test]
fn process_probe_error_never_proves_desktop_exit() {
    assert!(codex_app_pids_checked_with(|| Err("ps unavailable".into())).is_err());
    assert!(wait_for_app_exit_with(
        || Err("ps unavailable".into()),
        Duration::ZERO,
        Duration::ZERO,
    )
    .is_err());
}

#[test]
fn writer_started_after_snapshot_still_blocks_auth_replacement() {
    assert_eq!(
        desktop_writers_running_with(|| Ok(false), || Ok(true)),
        Ok(true)
    );
    assert!(desktop_writers_running_with(|| Ok(false), || Err("ps unavailable".into())).is_err());
    assert_eq!(
        desktop_writers_running_with(|| Ok(false), || Ok(false)),
        Ok(false)
    );
}

#[test]
fn multiwindow_inventory_blocks_shutdown_before_signalling() {
    let old = crate::distribution::WindowProcessIdentity::new(4242, "123:456").unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(
        signal_after_validated_snapshot(&[4242], &old, &old, 2, &[4242], |_| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls.get(), 0, "multiwindow shutdown must not send SIGTERM");
    assert!(validate_shutdown_snapshot(&[4242], &old, &old, 0, &[4242]).is_ok());
    assert!(validate_shutdown_snapshot(&[4242], &old, &old, 1, &[4242]).is_ok());
    signal_after_validated_snapshot(&[4242], &old, &old, 1, &[4242], |_| {
        calls.set(calls.get() + 1);
        Ok(())
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
}

#[test]
fn changed_or_multiple_desktop_processes_block_shutdown() {
    let old = crate::distribution::WindowProcessIdentity::new(4242, "123:456").unwrap();
    let calls = std::cell::Cell::new(0);
    assert!(validate_shutdown_snapshot(&[4242], &old, &old, 1, &[4243]).is_err());
    assert!(validate_shutdown_snapshot(&[4242, 4243], &old, &old, 1, &[4242, 4243]).is_err());
    assert!(validate_shutdown_snapshot(&[4242], &old, &old, 1, &[]).is_err());
    let recycled = crate::distribution::WindowProcessIdentity::new(4242, "123:457").unwrap();
    assert!(validate_shutdown_snapshot(&[4242], &old, &recycled, 1, &[4242]).is_err());
    assert!(
        signal_after_validated_snapshot(&[4242], &old, &recycled, 1, &[4242], |_| {
            calls.set(calls.get() + 1);
            Ok(())
        })
        .is_err()
    );
    assert_eq!(calls.get(), 0, "recycled PID must not receive SIGTERM");
}
