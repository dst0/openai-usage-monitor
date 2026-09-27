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
