use serde::{Deserialize, Serialize};

use super::rate_limit::RateLimit;
use super::rate_limit_reset_credits::RateLimitResetCredits;

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
