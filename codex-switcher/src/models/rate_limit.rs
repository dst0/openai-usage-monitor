use serde::{Deserialize, Serialize};

use super::primary_window::PrimaryWindow;
use super::secondary_window::SecondaryWindow;

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
