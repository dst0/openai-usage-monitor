use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RateLimitResetCredits {
    #[serde(default)]
    pub available_count: u32,
    #[serde(default)]
    pub applicable_available_count: Option<u32>,
}
