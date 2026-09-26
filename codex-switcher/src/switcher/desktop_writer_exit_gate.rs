use super::codex_process_probe::desktop_process_rows_checked;
use crate::distribution::{
    SystemWindowRestoreBackend, WindowProcessIdentity, WindowProcessValidationService,
};

const DESKTOP_BUNDLE_PREFIX: &str = "/Applications/ChatGPT.app/Contents/";

pub(super) struct DesktopWriterExitGate {
    writers: Vec<WindowProcessIdentity>,
}

impl DesktopWriterExitGate {
    pub(super) fn capture_with(
        rows: &str,
        main_pid: u32,
        mut inspect: impl FnMut(u32) -> Result<WindowProcessIdentity, String>,
    ) -> Result<Self, String> {
        if main_pid <= 1 {
            return Err("Desktop writer capture requires a valid main PID".into());
        }
        let mut writers = Vec::new();
        for (pid, parent, executable) in parse_process_rows(rows)? {
            if parent == main_pid
                && executable.starts_with(DESKTOP_BUNDLE_PREFIX)
                && executable.ends_with("/codex")
            {
                if pid <= 1 {
                    return Err("Desktop writer inspection returned an invalid writer PID".into());
                }
                let identity = inspect(pid)?;
                if identity.pid != pid {
                    return Err("Desktop writer identity has a different PID".into());
                }
                writers.push(identity);
            }
        }
        Ok(Self { writers })
    }

    pub(super) fn capture(main_pid: u32) -> Result<Self, String> {
        let rows = desktop_process_rows_checked()?;
        let mut backend = SystemWindowRestoreBackend::new()?;
        Self::capture_with(&rows, main_pid, |pid| {
            WindowProcessValidationService::inspect(&mut backend, pid)
        })
    }

    pub(super) fn writers_running_with(
        &self,
        rows: &str,
        mut inspect: impl FnMut(u32) -> Result<WindowProcessIdentity, String>,
    ) -> Result<bool, String> {
        let rows = parse_process_rows(rows)?;
        for expected in &self.writers {
            if rows.iter().any(|row| row.0 == expected.pid) {
                let current = inspect(expected.pid)?;
                if current == *expected {
                    return Ok(true);
                }
            }
        }
        Ok(false)
    }

    pub(super) fn writers_running_checked(&self) -> Result<bool, String> {
        if self.writers.is_empty() {
            return Ok(false);
        }
        let rows = desktop_process_rows_checked()?;
        let mut backend = SystemWindowRestoreBackend::new()?;
        self.writers_running_with(&rows, |pid| {
            WindowProcessValidationService::inspect(&mut backend, pid)
        })
    }
}

fn parse_process_rows(rows: &str) -> Result<Vec<(u32, u32, &str)>, String> {
    rows.lines()
        .map(|line| {
            let (pid, rest) = take_process_number(line.trim_start())?;
            let (parent, executable) = take_process_number(rest.trim_start())?;
            let executable = executable.trim();
            if executable.is_empty() {
                return Err("Desktop writer inspection returned an invalid row".into());
            }
            Ok((pid, parent, executable))
        })
        .collect()
}

fn take_process_number(row: &str) -> Result<(u32, &str), String> {
    let split = row
        .find(char::is_whitespace)
        .ok_or("Desktop writer inspection returned an invalid row")?;
    let value = row[..split]
        .parse::<u32>()
        .map_err(|_| "Desktop writer inspection returned an invalid PID")?;
    Ok((value, &row[split..]))
}

#[cfg(test)]
#[path = "desktop_writer_exit_gate.test.rs"]
mod tests;
