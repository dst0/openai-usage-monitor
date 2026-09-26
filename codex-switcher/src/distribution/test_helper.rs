use super::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use crate::models::{AccountConfig, AccountsFile, AuthJson, Settings};
use crate::storage::test_codex_home::TestCodexHome;
use crate::storage::{save_accounts, write_active_auth_json};
use std::path::Path;

/// A seeded distribution home. It owns the test's `CODEX_HOME` guard, so a
/// test must not also lock `TEST_CODEX_HOME_MUTEX` or create another guard.
pub struct TestEnv {
    home: TestCodexHome,
}

impl TestEnv {
    pub fn new(prefix: &str) -> Self {
        let home = TestCodexHome::new(&format!("dist-{prefix}"));
        MonitorLogLifecycleLock::ensure(home.path())
            .expect("test log lifecycle lock must initialize");
        Self { home }
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
                    extra: Default::default(),
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
            let _ = session.save(&self.home().join("desktop-app-session.json"));
        }
    }

    pub fn log_content(&self) -> String {
        let path = self.home().join("log").join("switcher.log");
        std::fs::read_to_string(path).unwrap_or_default()
    }

    pub fn home(&self) -> &Path {
        self.home.path()
    }
}
