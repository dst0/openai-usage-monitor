use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AuthTokens {
    pub access_token: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub refresh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub account_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthJson {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<String>,
    #[serde(rename = "OPENAI_API_KEY", skip_serializing_if = "Option::is_none")]
    pub openai_api_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tokens: Option<AuthTokens>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrimaryWindow {
    #[serde(default)]
    pub used_percent: f64,
    #[serde(default)]
    pub limit_window_seconds: u64,
    #[serde(default)]
    pub reset_after_seconds: i64,
    #[serde(default)]
    pub reset_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SecondaryWindow {
    #[serde(default)]
    pub used_percent: f64,
    #[serde(default)]
    pub limit_window_seconds: u64,
    #[serde(default)]
    pub reset_after_seconds: i64,
    #[serde(default)]
    pub reset_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimit {
    #[serde(default)]
    pub allowed: bool,
    #[serde(default)]
    pub limit_reached: bool,
    #[serde(default)]
    pub primary_window: Option<PrimaryWindow>,
    #[serde(default)]
    pub secondary_window: Option<SecondaryWindow>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResetCredits {
    #[serde(default)]
    pub available_count: u32,
    #[serde(default)]
    pub applicable_available_count: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WhamUsageResponse {
    #[serde(default)]
    pub user_id: Option<String>,
    #[serde(default)]
    pub account_id: Option<String>,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub plan_type: Option<String>,
    #[serde(default)]
    pub rate_limit: Option<RateLimit>,
    #[serde(default)]
    pub rate_limit_reset_credits: Option<RateLimitResetCredits>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OAuthTokenResponse {
    pub access_token: String,
    #[serde(default)]
    pub refresh_token: Option<String>,
    #[serde(default)]
    pub id_token: Option<String>,
    #[serde(default)]
    pub token_type: Option<String>,
    #[serde(default)]
    pub expires_in: Option<u64>,
}

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
}

impl AccountConfig {
    pub fn display_name(&self) -> &str {
        if let Some(ref n) = self.name {
            if !n.trim().is_empty() {
                return n.trim();
            }
        }
        let email_prefix = self.email.split('@').next().unwrap_or(&self.email);
        if !email_prefix.is_empty() {
            email_prefix
        } else {
            &self.id
        }
    }

    pub fn effective_multiplier(&self) -> f64 {
        if let Some(m) = self.plan_multiplier {
            if m > 0.0 {
                return m;
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

    #[allow(dead_code)]
    pub fn effective_percentage(&self) -> f64 {
        self.last_primary_percentage * self.effective_multiplier()
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Settings {
    #[serde(default)]
    pub switch_threshold_percent: f64,
    #[serde(default = "default_poll_interval")]
    pub poll_interval_seconds: u64,
    #[serde(default)]
    pub restart_app_on_switch: bool,
    #[serde(default = "default_true")]
    pub notify_on_switch: bool,
    #[serde(default = "default_strategy")]
    pub strategy: String,
    #[serde(default = "default_true")]
    pub auto_switch_enabled: bool,
}

fn default_poll_interval() -> u64 {
    60
}
fn default_strategy() -> String {
    "reset-first".to_string()
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            switch_threshold_percent: 0.0,
            poll_interval_seconds: 60,
            restart_app_on_switch: false,
            notify_on_switch: true,
            strategy: "reset-first".to_string(),
            auto_switch_enabled: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AccountsFile {
    pub active_account_id: Option<String>,
    #[serde(default)]
    pub settings: Settings,
    #[serde(default)]
    pub accounts: Vec<AccountConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AccountStatusEntry {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    pub email: String,
    pub plan_type: String,
    pub is_active: bool,
    pub five_hour_percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_after_seconds: Option<i64>,
    pub credits: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default = "default_1_0")]
    pub plan_multiplier: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatusFile {
    pub timestamp: String,
    pub active_account_id: Option<String>,
    pub active_email: Option<String>,
    pub active_plan: Option<String>,
    pub five_hour_percentage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_percentage: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_after_seconds: Option<i64>,
    pub credits: u32,
    pub auto_switch_enabled: bool,
    #[serde(default = "default_1_0")]
    pub plan_multiplier: f64,
    pub accounts: Vec<AccountStatusEntry>,
}

fn default_1_0() -> f64 {
    1.0
}
