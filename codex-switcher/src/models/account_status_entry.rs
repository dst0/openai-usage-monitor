use serde::{Deserialize, Serialize};

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
    pub weekly_reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub weekly_reset_after_seconds: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_time: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reset_after_seconds: Option<i64>,
    pub credits: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
    #[serde(default = "default_1_0")]
    pub plan_multiplier: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub organization_name: Option<String>,
}

fn default_1_0() -> f64 {
    1.0
}
