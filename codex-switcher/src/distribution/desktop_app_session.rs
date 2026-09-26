use super::window_restore_process_identity::ProcessIdentity;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DesktopAppSession {
    pub account_id: String,
    pub updated_at: String,
    #[serde(default)]
    pub process: Option<ProcessIdentity>,
    #[serde(default)]
    pub cli_account_id: Option<String>,
}

impl DesktopAppSession {
    pub fn new(account_id: impl Into<String>) -> Self {
        Self {
            account_id: account_id.into(),
            updated_at: Utc::now().to_rfc3339(),
            process: None,
            cli_account_id: None,
        }
    }

    pub fn bound(
        account_id: impl Into<String>,
        cli_account_id: impl Into<String>,
        process: ProcessIdentity,
    ) -> Self {
        let mut session = Self::new(account_id);
        session.process = Some(process);
        session.cli_account_id = Some(cli_account_id.into());
        session
    }

    pub fn load(path: &Path) -> Option<Self> {
        let content = std::fs::read_to_string(path).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let data = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension(format!("{}.tmp", std::process::id()));
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
}
