use super::{BannerSessionStatus, RecoverySession};
use serde_json::Value;
use std::path::Path;
use std::process::Command;

/// Reads only the non-sensitive display metadata required by the banner.
pub struct RecoverySessionCatalog;

impl RecoverySessionCatalog {
    pub fn load(home: &Path, ids: &[String]) -> Vec<RecoverySession> {
        ids.iter()
            .map(|id| {
                let (project, title) = Self::metadata(home, id)
                    .unwrap_or_else(|| ("Codex".to_string(), "Восстановление задачи".to_string()));
                RecoverySession::from_raw(project, title, id, BannerSessionStatus::Pending)
            })
            .collect()
    }

    fn metadata(home: &Path, id: &str) -> Option<(String, String)> {
        if id.len() != 36
            || !id
                .bytes()
                .all(|byte| byte.is_ascii_hexdigit() || byte == b'-')
        {
            return None;
        }
        let sql = format!(
            "SELECT json_object('cwd', cwd, 'title', title) FROM threads WHERE id = '{id}' AND archived = 0 LIMIT 1;"
        );
        let output = Command::new("sqlite3")
            .args([
                "-noheader",
                "-batch",
                home.join("state_5.sqlite").to_str()?,
                &sql,
            ])
            .output()
            .ok()?;
        if !output.status.success() {
            return None;
        }
        let value: Value =
            serde_json::from_str(String::from_utf8_lossy(&output.stdout).trim()).ok()?;
        let project = value["cwd"].as_str()?.to_string();
        let title = value["title"].as_str().unwrap_or("").to_string();
        Some((project, title))
    }
}
