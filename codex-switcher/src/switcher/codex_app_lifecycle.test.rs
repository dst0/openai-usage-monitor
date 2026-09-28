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
fn failed_final_task_check_never_signals_desktop() {
    let signals = std::cell::Cell::new(0);
    let window_checks = std::cell::Cell::new(0);
    let result = verify_then_signal(
        || Err("selected task changed".into()),
        || {
            window_checks.set(window_checks.get() + 1);
            Ok(())
        },
        || {
            signals.set(signals.get() + 1);
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err(), "selected task changed");
    assert_eq!(window_checks.get(), 0);
    assert_eq!(signals.get(), 0);
}

#[test]
fn failed_final_window_check_never_signals_desktop() {
    let signals = std::cell::Cell::new(0);
    let result = verify_then_signal(
        || Ok(()),
        || Err("window list changed".into()),
        || {
            signals.set(signals.get() + 1);
            Ok(())
        },
    );
    assert_eq!(result.unwrap_err(), "window list changed");
    assert_eq!(signals.get(), 0);
}
