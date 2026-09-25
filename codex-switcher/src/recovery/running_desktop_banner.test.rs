use super::verify_helper_running;
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
