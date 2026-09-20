use serde::{Deserialize, Serialize};

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
