use super::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use crate::models::{AccountConfig, AccountsFile, AuthJson, AuthTokens, Settings};
use crate::storage::{save_accounts, write_active_auth_json};
use std::path::{Path, PathBuf};

pub fn make_account(
    id: &str,
    name: Option<&str>,
    email: &str,
    plan: &str,
    sprint_pct: f64,
    weekly_pct: Option<f64>,
    credits: u32,
    reset_after: Option<i64>,
    error: Option<&str>,
) -> AccountConfig {
    AccountConfig {
        id: id.to_string(),
        name: name.map(ToString::to_string),
        email: email.to_string(),
        plan_type: plan.to_string(),
        account_id: id.to_string(),
        tokens: AuthTokens {
            access_token: format!("tok_{id}"),
            refresh_token: Some(format!("rt_{id}")),
            id_token: None,
            account_id: Some(id.to_string()),
        },
        enabled: true,
        priority: 0,
        last_primary_percentage: sprint_pct,
        last_reset_time: None,
        last_reset_after_seconds: reset_after,
        last_weekly_percentage: weekly_pct,
        last_weekly_reset_time: None,
        last_weekly_reset_after_seconds: None,
        last_credits: Some(credits),
        last_error: error.map(ToString::to_string),
        last_checked: None,
        plan_multiplier: None,
        multiplier_is_manual: None,
        last_multiplier_checked: None,
        organization_name: None,
    }
}

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
        let mut settings = Settings::default();
        settings.auto_switch_enabled = true;
        settings.auto_switch_business_priority = true;

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
            let canonical_app_id = accounts_file
                .accounts
                .iter()
                .find(|account| account.id == app_id)
                .map(|account| {
                    crate::setup::build_predictable_account_id(&account.email, &account.account_id)
                })
                .unwrap_or_else(|| app_id.to_string());
            let canonical_cli_id = active_cli
                .and_then(|cli_id| {
                    accounts_file
                        .accounts
                        .iter()
                        .find(|account| account.id == cli_id)
                        .map(|account| {
                            crate::setup::build_predictable_account_id(
                                &account.email,
                                &account.account_id,
                            )
                        })
                })
                .unwrap_or_else(|| canonical_app_id.clone());
            let process =
                super::window_restore_process_identity::ProcessIdentity::new(9999, "123:456789")
                    .unwrap();
            let session = super::desktop_app_session::DesktopAppSession::bound(
                canonical_app_id,
                canonical_cli_id,
                process,
            );
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
