use serde::{Deserialize, Serialize};

use super::account_status_entry::AccountStatusEntry;

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
    pub weekly_reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_reset_after_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_after_seconds: Option<i64>,
    pub credits: u32,
    pub auto_switch_enabled: bool,
    #[serde(default)]
    pub auto_switch_business_only: bool,
    #[serde(default)]
    pub auto_switch_business_priority: bool,
    #[serde(default)]
    pub auto_reset_weekly_enabled: bool,
    #[serde(default)]
    pub auto_reset_weekly_min_remaining_seconds: u64,
    #[serde(default = "default_auto_reset_state")]
    pub auto_reset_state: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_reset_reason: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auto_reset_last_event_at: Option<String>,
    #[serde(default = "default_1_0")]
    pub plan_multiplier: f64,
    pub accounts: Vec<AccountStatusEntry>,
}

fn default_auto_reset_state() -> String {
    "disabled".to_string()
}

fn default_1_0() -> f64 {
    1.0
}
