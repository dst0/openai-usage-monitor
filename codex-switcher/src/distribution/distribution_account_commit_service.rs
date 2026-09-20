use crate::models::{AccountConfig, AccountsFile, AuthJson};
use crate::storage::{read_active_auth_json, write_active_auth_json};
use chrono::Utc;

pub struct DistributionAccountCommitService;

impl DistributionAccountCommitService {
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
}
