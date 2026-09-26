use serde::{Deserialize, Serialize};

use super::auth_tokens::AuthTokens;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY", skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<AuthTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<String>,
    #[serde(default, flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}
