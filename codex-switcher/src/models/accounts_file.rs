use serde::{Deserialize, Serialize};

use super::account_config::AccountConfig;
use super::settings::Settings;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountsFile {
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
}
