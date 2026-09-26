use super::codex_app_lifecycle::CODEX_APP_EXECUTABLE;
use super::thread_identity::clean_thread_id;
use chrono::Utc;
use std::process::Command;

/// Owns the detached restart worker and validates its parentage before dispatch.
pub(super) struct RestartWorkerDispatchService;

impl RestartWorkerDispatchService {
    /// Self-restart is supported: a detached worker, rather than the app's child
    /// shell, owns the operation so recovery survives termination of this host.
    pub(super) fn dispatch(args: &[String]) -> Result<bool, String> {
        if std::env::var_os("CODEX_RESTART_WORKER").is_some() {
            return Ok(false);
        }
        let output = Command::new("/bin/ps")
            .args(["-axo", "pid=,ppid=,comm="])
            .output()
            .map_err(|e| e.to_string())?;
        if !output.status.success() {
            return Err("Cannot verify restart worker ancestry".into());
        }
        if !Self::has_codex_ancestor(&String::from_utf8_lossy(&output.stdout), std::process::id())?
        {
            return Ok(false);
        }
        let home = crate::storage::codex_home();
        let label = "com.codex.switcher.restart-worker";
        let previous = Command::new("launchctl")
            .args(["list", label])
            .output()
            .map_err(|e| e.to_string())?;
        if previous.status.success() {
            if String::from_utf8_lossy(&previous.stdout).contains("\"PID\"") {
                return Err("A restart worker is already running".into());
            }
            let removed = Command::new("launchctl")
                .args(["remove", label])
                .status()
                .map_err(|e| e.to_string())?;
            if !removed.success() {
                return Err("Could not retire the completed restart worker".into());
            }
        }
        crate::recovery::arm_automation_cooldown()?;
        crate::recovery::clear_restart_cancellation()?;
        let directory = home.join("recovery-runs");
        std::fs::create_dir_all(&directory).map_err(|e| e.to_string())?;
        let operation_id = format!("{}-{}", Utc::now().timestamp_millis(), std::process::id());
        let log = directory.join(format!("restart-{operation_id}.log"));
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(&log)
            .map_err(|e| e.to_string())?;
        let executable = std::env::current_exe().map_err(|e| e.to_string())?;
        let mut submit = Command::new("launchctl");
        submit
        .args(["submit", "-l", label, "-o"])
        .arg(&log)
        .arg("-e")
        .arg(&log)
        // A submitted job can be relaunched even after a short successful run.
        // Remove the one-shot label from inside the wrapper after the child
        // finishes. The operation claim remains the destructive at-most-once
        // guard if the wrapper itself is interrupted before that cleanup.
        .args([
            "--",
            "/bin/sh",
            "-c",
            "\"$@\"; /bin/launchctl remove com.codex.switcher.restart-worker >/dev/null 2>&1; exit 0",
            "codex-restart-once",
            "/usr/bin/env",
            "CODEX_RESTART_WORKER=1",
        ])
        .arg(format!("CODEX_RESTART_OPERATION={operation_id}"))
        .arg(format!("CODEX_HOME={}", home.display()));
        if let Ok(primary) = std::env::var("CODEX_THREAD_ID") {
            submit.arg(format!(
                "CODEX_PRIMARY_THREAD={}",
                clean_thread_id(&primary)
            ));
        }
        let status = submit
            .arg(executable)
            .arg("--restart-worker")
            .args(args)
            .status()
            .map_err(|e| e.to_string())?;
        if !status.success() {
            return Err("Could not launch independent restart worker".into());
        }
        crate::runtime_print!(
            "RESTART_DISPATCHED job={label} log={} (scheduled, not yet verified)",
            log.display()
        );
        Ok(true)
    }

    pub(super) fn has_codex_ancestor(processes: &str, mut pid: u32) -> Result<bool, String> {
        let rows: Vec<_> = processes
            .lines()
            .filter_map(|line| {
                let mut fields = line.trim().splitn(3, char::is_whitespace);
                let id = fields.next()?.parse::<u32>().ok()?;
                let rest = line.trim().strip_prefix(&id.to_string())?.trim_start();
                let split = rest.find(char::is_whitespace)?;
                Some((id, rest[..split].parse::<u32>().ok()?, rest[split..].trim()))
            })
            .collect();
        for _ in 0..128 {
            if pid <= 1 {
                return Ok(false);
            }
            let (_, parent, executable) = rows
                .iter()
                .find(|row| row.0 == pid)
                .ok_or("Cannot resolve restart worker ancestry")?;
            if *executable == CODEX_APP_EXECUTABLE {
                return Ok(true);
            }
            pid = *parent;
        }
        Err("Cycle in restart worker ancestry".into())
    }
}
