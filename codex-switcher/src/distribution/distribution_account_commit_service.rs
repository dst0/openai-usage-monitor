use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{read_active_auth_json, save_accounts, write_active_auth_json};
use chrono::Utc;

pub struct DistributionAccountCommitService;

impl DistributionAccountCommitService {
    pub fn validate_targets(
        accounts_file: &AccountsFile,
        app_target: Option<&str>,
        cli_target: Option<&str>,
    ) -> Result<(), String> {
        for target in [app_target, cli_target].into_iter().flatten() {
            Self::find_account(accounts_file, target)?;
        }
        Ok(())
    }

    pub fn find_account(
        accounts_file: &AccountsFile,
        query: &str,
    ) -> Result<AccountConfig, String> {
        let index = crate::switcher::resolve_target_account_idx(&accounts_file.accounts, query)?;
        Ok(accounts_file.accounts[index].clone())
    }

    pub fn apply_auth_tokens(account: &AccountConfig) -> Result<(), String> {
        let mut auth = read_active_auth_json().unwrap_or(AuthJson {
            auth_mode: Some("chatgpt".to_string()),
            openai_api_key: None,
            tokens: None,
            last_refresh: None,
        });
        auth.tokens = Some(account.tokens.clone());
        auth.last_refresh = Some(Utc::now().to_rfc3339());
        write_active_auth_json(&auth)
    }

    /// Commit CLI credentials and registry together, restoring auth if the
    /// second file cannot be saved. The caller still holds the operation lock.
    pub fn commit_cli_account(
        accounts_file: &mut AccountsFile,
        account: &AccountConfig,
    ) -> Result<(), String> {
        let previous_auth = read_active_auth_json()?;
        let previous_active = accounts_file.active_account_id.clone();
        Self::apply_auth_tokens(account)?;
        accounts_file.active_account_id = Some(account.id.clone());
        if let Err(error) = save_accounts(accounts_file) {
            accounts_file.active_account_id = previous_active;
            if let Err(restore_error) = write_active_auth_json(&previous_auth) {
                return Err(format!(
                    "CLI registry commit failed; restoring authentication also failed: {restore_error}"
                ));
            }
            return Err(format!("CLI registry commit failed: {error}"));
        }
        Ok(())
    }
}
