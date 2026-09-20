use serde::{Deserialize, Serialize};

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
    #[serde(default)]
    pub auto_switch_business_only: bool,
    #[serde(default)]
    pub auto_switch_business_priority: bool,
    #[serde(default)]
    pub auto_reset_weekly_enabled: bool,
    #[serde(default)]
    pub auto_reset_weekly_min_remaining_seconds: u64,
    #[serde(default = "default_true")]
    pub preserve_window_bounds_on_restart: bool,
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
            auto_switch_business_only: false,
            auto_switch_business_priority: false,
            auto_reset_weekly_enabled: false,
            auto_reset_weekly_min_remaining_seconds: 0,
            preserve_window_bounds_on_restart: true,
        }
    }
}

fn default_poll_interval() -> u64 {
    60
}

fn default_strategy() -> String {
    "reset-first".to_string()
}

fn default_true() -> bool {
    true
}
