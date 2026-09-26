use super::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use crate::models::{AccountConfig, AccountsFile, AuthJson, Settings};
use crate::storage::{save_accounts, write_active_auth_json};
use std::path::{Path, PathBuf};

pub struct TestEnv {
    pub dir: PathBuf,
}

impl TestEnv {
    pub fn new(prefix: &str) -> Self {
        let unique = chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0);
        let dir = std::env::temp_dir().join(format!(
            "codex_dist_test_{prefix}_{}_{unique}",
            std::process::id()
        ));
        let _ = std::fs::create_dir_all(&dir);
        let dir = std::fs::canonicalize(dir).expect("test home must canonicalize");
        std::env::set_var("CODEX_HOME", &dir);
        MonitorLogLifecycleLock::ensure(&dir).expect("test log lifecycle lock must initialize");
        Self { dir }
    }

    pub fn populate(
        &self,
        accounts: Vec<AccountConfig>,
        active_cli: Option<&str>,
        active_app: Option<&str>,
    ) {
        let settings = Settings {
            auto_switch_enabled: true,
            auto_switch_business_priority: true,
            ..Settings::default()
        };

        let accounts_file = AccountsFile {
            active_account_id: active_cli.map(ToString::to_string),
            settings,
            accounts,
        };
        let _ = save_accounts(&accounts_file);

        if let Some(cli_id) = active_cli {
            if let Some(acc) = accounts_file.accounts.iter().find(|a| a.id == cli_id) {
                let auth = AuthJson {
                    auth_mode: Some("chatgpt".to_string()),
                    openai_api_key: None,
                    tokens: Some(acc.tokens.clone()),
                    last_refresh: None,
                };
                let _ = write_active_auth_json(&auth);
            }
        }

        if let Some(app_id) = active_app {
            let session = super::desktop_app_session::DesktopAppSession::new(app_id);
            let _ = session.save(&self.dir.join("desktop-app-session.json"));
        }
    }

    pub fn log_content(&self) -> String {
        let path = self.dir.join("log").join("switcher.log");
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub fn home(&self) -> &Path {
        &self.dir
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.dir);
    }
}
