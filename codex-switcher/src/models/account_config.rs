use serde::{Deserialize, Serialize};

use super::auth_tokens::AuthTokens;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountConfig {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub email: String,
    #[serde(default = "default_plan")]
    pub plan_type: String,
    pub account_id: String,
    pub tokens: AuthTokens,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub priority: i32,
    #[serde(default = "default_100")]
    pub last_primary_percentage: f64,
    #[serde(default)]
    pub last_reset_time: Option<String>,
    #[serde(default)]
    pub last_reset_after_seconds: Option<i64>,
    #[serde(default)]
    pub last_weekly_percentage: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_weekly_reset_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_weekly_reset_after_seconds: Option<i64>,
    #[serde(default)]
    pub last_credits: Option<u32>,
    #[serde(default)]
    pub last_error: Option<String>,
    #[serde(default)]
    pub last_checked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub plan_multiplier: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub multiplier_is_manual: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_multiplier_checked: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_name: Option<String>,
}

impl AccountConfig {
    pub fn display_name(&self) -> &str {
        if let Some(ref name) = self.name {
            if !name.trim().is_empty() {
                return name.trim();
            }
        }
        let email_prefix = self.email.split('@').next().unwrap_or(&self.email);
        if !email_prefix.is_empty() {
            email_prefix
        } else {
            &self.id
        }
    }

    pub fn needs_relogin(&self) -> bool {
        if self.tokens.access_token.trim().is_empty() {
            return true;
        }
        if let Some(ref error) = self.last_error {
            let lower = error.to_ascii_lowercase();
            return lower.contains("401")
                || lower.contains("re-login")
                || lower.contains("relogin")
                || lower.contains("session ended")
                || lower.contains("logged out")
                || lower.contains("unauthorized")
                || lower.contains("token_revoked")
                || lower.contains("invalid_grant")
                || lower.contains("refresh_token_invalidated");
        }
        false
    }

    pub fn effective_multiplier(&self) -> f64 {
        if let Some(multiplier) = self.plan_multiplier {
            if multiplier > 0.0 {
                return multiplier;
            }
        }
        match self.plan_type.to_lowercase().as_str() {
            "pro" => 20.0,
            "team" | "business" => 1.0,
            "plus" => 1.0,
            "free" => 0.2,
            _ => 1.0,
        }
    }

    pub fn is_business(&self) -> bool {
        let plan = self.plan_type.trim().to_lowercase();
        if plan == "business" || plan == "team" || plan == "enterprise" {
            return true;
        }
        if let Some(ref name) = self.name {
            let lower = name.trim().to_lowercase();
            if lower == "business"
                || lower == "team"
                || lower.contains("business")
                || lower.contains("corporate")
            {
                return true;
            }
        }
        let id = self.id.to_lowercase();
        id.contains("business") || id.contains("-[business]")
    }
}

fn default_plan() -> String {
    "team".to_string()
}

fn default_true() -> bool {
    true
}

fn default_100() -> f64 {
    100.0
}
