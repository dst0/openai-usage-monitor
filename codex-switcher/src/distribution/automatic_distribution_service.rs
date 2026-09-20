use super::automatic_distribution_source::AutomaticDistributionSource;
use super::distribution_executor::DistributionExecutor;
use super::distribution_outcome::DistributionOutcome;
use super::distribution_request::DistributionRequest;
use crate::models::{AccountConfig, AccountsFile};
use crate::strategy::{is_account_depleted, needs_switch};

pub struct AutomaticDistributionService<'a, E: DistributionExecutor> {
    executor: &'a E,
}

impl<'a, E: DistributionExecutor> AutomaticDistributionService<'a, E> {
    pub fn new(executor: &'a E) -> Self {
        Self { executor }
    }

    pub fn execute(
        &self,
        source: AutomaticDistributionSource,
        accounts_file: &AccountsFile,
        suppressed: bool,
    ) -> Result<Option<DistributionOutcome>, String> {
        let Some(request) = Self::request(source, accounts_file, suppressed) else {
            return Ok(None);
        };
        self.executor.execute(request).map(Some)
    }

    fn request(
        source: AutomaticDistributionSource,
        accounts_file: &AccountsFile,
        suppressed: bool,
    ) -> Option<DistributionRequest> {
        if suppressed || !accounts_file.settings.auto_switch_enabled {
            return None;
        }

        let active = active_account(accounts_file)?;
        let threshold = accounts_file.settings.switch_threshold_percent;
        let business_priority = accounts_file.settings.auto_switch_business_priority;
        if !needs_switch(
            active,
            threshold,
            business_priority,
            &accounts_file.accounts,
        ) {
            return None;
        }

        let cause = classify_cause(active, threshold, business_priority);
        Some(
            DistributionRequest::auto(source.reason(cause))
                .with_allow_restart(accounts_file.settings.restart_app_on_switch),
        )
    }
}

fn active_account(accounts_file: &AccountsFile) -> Option<&AccountConfig> {
    let active_id = accounts_file.active_account_id.as_deref()?;
    accounts_file
        .accounts
        .iter()
        .find(|account| account.id == active_id || account.email.eq_ignore_ascii_case(active_id))
}

fn classify_cause(active: &AccountConfig, threshold: f64, business_priority: bool) -> &'static str {
    if active
        .last_error
        .as_deref()
        .is_some_and(is_rate_limit_error)
    {
        return "rate_limit";
    }
    if active
        .last_weekly_percentage
        .is_some_and(|weekly| weekly <= threshold)
        && active.last_credits.unwrap_or(0) == 0
    {
        return "weekly_quota_exhausted";
    }
    if active.last_primary_percentage <= threshold {
        return "quota_exhausted";
    }
    if is_account_depleted(active, threshold) {
        return "account_unavailable";
    }
    if business_priority && !active.is_business() {
        return "business_priority_return";
    }
    "automatic_policy"
}

fn is_rate_limit_error(error: &str) -> bool {
    let error = error.to_ascii_lowercase();
    error.contains("429")
        || error.contains("rate limit")
        || error.contains("rate_limit")
        || error.contains("usage_limit_exceeded")
        || error.contains("credits_depleted")
        || error.contains("out of credits")
        || error.contains("quota")
}
