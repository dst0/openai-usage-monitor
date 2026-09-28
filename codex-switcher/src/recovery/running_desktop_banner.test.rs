use super::verify_helper_running;
use crate::distribution::WindowProcessIdentity;
use crate::recovery::RecoveryBanner;
use std::process::Command;

#[test]
fn banner_helper_must_still_be_alive_before_ipc_dispatch() {
    let mut exited = Command::new("/usr/bin/true").spawn().unwrap();
    exited.wait().unwrap();
    assert!(verify_helper_running(&mut exited).is_err());

    let mut running = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    assert!(verify_helper_running(&mut running).is_ok());
    running.kill().unwrap();
    running.wait().unwrap();
}

#[test]
fn pending_mount_panel_rejects_new_desktop_lifetime() {
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let changed = WindowProcessIdentity::new(4243, "1726789012:000008").unwrap();
    let mut banner = RecoveryBanner::without_window(process.clone());
    assert!(!banner
        .try_show_pending_for_mount_with(|| Ok(RecoveryBanner::without_window(process)))
        .unwrap());
    assert!(banner
        .try_show_pending_for_mount_with(|| Ok(RecoveryBanner::without_window(changed)))
        .is_err());
    assert!(!banner.has_visible_panel());
}
