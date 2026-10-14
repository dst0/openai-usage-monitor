use std::io;
use std::path::Path;
use std::process::{Child, Command, Stdio};

/// macOS's background task policy throttles CPU and disk I/O for the pass.
const TASKPOLICY: &str = "/usr/sbin/taskpolicy";

/// Starts `codex-mon sessions compress --scheduled` as a detached,
/// background-priority child of the daemon. Its summary goes to the Monitor
/// log; its own output is discarded.
pub fn spawn_scheduled_compression() -> io::Result<Child> {
    let executable = std::env::current_exe()?;
    let mut command = if Path::new(TASKPOLICY).is_file() {
        let mut command = Command::new(TASKPOLICY);
        command.arg("-b").arg(&executable);
        command
    } else {
        Command::new(&executable)
    };
    command
        .args(["sessions", "compress", "--scheduled"])
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
}
