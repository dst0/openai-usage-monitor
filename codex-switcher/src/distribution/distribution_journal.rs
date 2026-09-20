use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub const JOURNAL_FILENAME: &str = "distribution-journal.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DistributionJournal {
    pub operation_id: String,
    pub pid: u32,
    pub trigger: String,
    pub reason: String,
    pub target_app_id: Option<String>,
    pub target_cli_id: Option<String>,
    pub phase: String,
    pub started_at: String,
    pub updated_at: String,
}

impl DistributionJournal {
    pub fn journal_path(home: &Path) -> PathBuf {
        home.join(JOURNAL_FILENAME)
    }

    pub fn create(
        home: &Path,
        operation_id: &str,
        trigger: &str,
        reason: &str,
        target_app: Option<&str>,
        target_cli: Option<&str>,
    ) -> Result<Self, String> {
        let now = Utc::now().to_rfc3339();
        let journal = Self {
            operation_id: operation_id.to_string(),
            pid: std::process::id(),
            trigger: trigger.to_string(),
            reason: reason.to_string(),
            target_app_id: target_app.map(ToString::to_string),
            target_cli_id: target_cli.map(ToString::to_string),
            phase: "initialized".to_string(),
            started_at: now.clone(),
            updated_at: now,
        };
        journal.save(home)?;
        Ok(journal)
    }

    pub fn load(home: &Path) -> Result<Option<Self>, String> {
        let path = Self::journal_path(home);
        let content = match std::fs::read_to_string(path) {
            Ok(content) => content,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(_) => return Err("Distribution journal is unreadable".to_string()),
        };
        serde_json::from_str(&content)
            .map(Some)
            .map_err(|_| "Distribution journal is invalid".to_string())
    }

    pub fn save(&self, home: &Path) -> Result<(), String> {
        std::fs::create_dir_all(home).map_err(|e| e.to_string())?;
        let path = Self::journal_path(home);
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = home.join(format!("distribution-journal.{}.tmp", std::process::id()));
        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp)
            .map_err(|e| e.to_string())?;
        file.write_all(data.as_bytes()).map_err(|e| e.to_string())?;
        file.sync_all().map_err(|e| e.to_string())?;
        std::fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn update_phase(&mut self, home: &Path, phase: &str) -> Result<(), String> {
        self.phase = phase.to_string();
        self.updated_at = Utc::now().to_rfc3339();
        self.save(home)
    }

    pub fn clear(home: &Path) -> Result<(), String> {
        let path = Self::journal_path(home);
        match std::fs::remove_file(path) {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn is_stale(&self, timeout: Duration) -> bool {
        // If the process is dead, it is definitely stale
        if !is_process_alive(self.pid) {
            return true;
        }
        // If elapsed time exceeds timeout, treat as stale
        if let Ok(started) = DateTime::parse_from_rfc3339(&self.started_at) {
            let elapsed = Utc::now().signed_duration_since(started.with_timezone(&Utc));
            if elapsed
                > chrono::Duration::from_std(timeout)
                    .unwrap_or_else(|_| chrono::Duration::seconds(300))
            {
                return true;
            }
        }
        false
    }
}

fn is_process_alive(pid: u32) -> bool {
    if pid == 0 {
        return false;
    }
    unsafe {
        // kill(pid, 0) returns 0 if process exists, or -1 with ESRCH if it does not
        if libc::kill(pid as i32, 0) == 0 {
            true
        } else {
            std::io::Error::last_os_error().raw_os_error() != Some(libc::ESRCH)
        }
    }
}
