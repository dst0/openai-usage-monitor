#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct DistributionCandidate {
    pub account_id: String,
    pub sanitized_label: String,
    pub plan_type: String,
    pub is_business: bool,
    pub five_hour_percentage: f64,
    pub weekly_percentage: Option<f64>,
    pub credits: u32,
    pub reset_after_seconds: Option<i64>,
    pub priority: i32,
    pub eligible: bool,
    pub skip_reason: Option<String>,
}

impl DistributionCandidate {
    #[allow(clippy::too_many_arguments)]
    pub fn eligible(
        account_id: String,
        sanitized_label: String,
        plan_type: String,
        is_business: bool,
        five_hour_percentage: f64,
        weekly_percentage: Option<f64>,
        credits: u32,
        reset_after_seconds: Option<i64>,
        priority: i32,
    ) -> Self {
        Self {
            account_id,
            sanitized_label,
            plan_type,
            is_business,
            five_hour_percentage,
            weekly_percentage,
            credits,
            reset_after_seconds,
            priority,
            eligible: true,
            skip_reason: None,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn ineligible(
        account_id: String,
        sanitized_label: String,
        plan_type: String,
        is_business: bool,
        five_hour_percentage: f64,
        weekly_percentage: Option<f64>,
        credits: u32,
        reset_after_seconds: Option<i64>,
        priority: i32,
        reason: impl Into<String>,
    ) -> Self {
        Self {
            account_id,
            sanitized_label,
            plan_type,
            is_business,
            five_hour_percentage,
            weekly_percentage,
            credits,
            reset_after_seconds,
            priority,
            eligible: false,
            skip_reason: Some(reason.into()),
        }
    }
}
