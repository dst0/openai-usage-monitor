use crate::models::AccountConfig;

pub fn is_quota_depleted(
    five_hour_percentage: f64,
    weekly_percentage: Option<f64>,
    credits: u32,
    error: Option<&str>,
    threshold: f64,
) -> bool {
    // 1. Primary 5-hour sprint reached or fell below threshold
    if five_hour_percentage <= threshold {
        return true;
    }

    // 2. Weekly quota reached or fell below threshold with no reset credits available
    if let Some(weekly) = weekly_percentage {
        if weekly <= threshold && credits == 0 {
            return true;
        }
    }

    // 3. Error indicates quota / rate limit exhaustion or fatal authentication error
    if let Some(err) = error {
        let lower = err.to_ascii_lowercase();
        if lower.contains("429")
            || lower.contains("401")
            || lower.contains("403")
            || lower.contains("unauthorized")
            || lower.contains("usage_limit_exceeded")
            || lower.contains("workspace_owner_credits_depleted")
            || lower.contains("credits_depleted")
            || lower.contains("out of credits")
            || lower.contains("rate limit")
            || lower.contains("rate_limit")
            || lower.contains("quota")
            || lower.contains("session ended")
            || lower.contains("logged out")
            || lower.contains("re-login")
            || lower.contains("relogin")
            || lower.contains("token_revoked")
            || lower.contains("invalid_grant")
            || lower.contains("refresh_token_invalidated")
        {
            return true;
        }
    }

    false
}

pub fn is_account_depleted(account: &AccountConfig, threshold: f64) -> bool {
    if account.needs_relogin() {
        return true;
    }
    is_quota_depleted(
        account.last_primary_percentage,
        account.last_weekly_percentage,
        account.last_credits.unwrap_or(0),
        account.last_error.as_deref(),
        threshold,
    )
}

pub fn needs_switch(
    account: &AccountConfig,
    threshold: f64,
    business_priority: bool,
    accounts: &[AccountConfig],
) -> bool {
    // 1. Normal exhaustion: active account reached or fell below threshold,
    // or weekly quota exhausted with 0 credits, or rate-limit error encountered.
    if is_account_depleted(account, threshold) {
        return true;
    }

    // 2. Preemption: if business_priority is enabled and active account is non-business,
    // switch if any enabled business account has available quota without errors.
    if business_priority && !account.is_business() {
        let has_available_business = accounts.iter().any(|a| {
            a.id != account.id
                && !a.email.eq_ignore_ascii_case(&account.email)
                && a.enabled
                && a.is_business()
                && !a.needs_relogin()
                && a.last_error
                    .as_deref()
                    .map(|s| s.trim().is_empty())
                    .unwrap_or(true)
                && !is_account_depleted(a, threshold)
        });
        if has_available_business {
            return true;
        }
    }

    false
}

pub fn select_best_switch(
    active_id: Option<&str>,
    accounts: &[AccountConfig],
    threshold: f64,
    strategy: &str,
    business_only: bool,
    business_priority: bool,
) -> Option<String> {
    // If only 1 account or none, switching is impossible and must never occur
    if accounts.len() <= 1 {
        return None;
    }

    let active = active_id.and_then(|id| {
        accounts
            .iter()
            .find(|a| a.id == id || a.email.eq_ignore_ascii_case(id))
    });
    let active_is_non_business = active.map(|a| !a.is_business()).unwrap_or(false);
    let active_depleted = active
        .map(|a| is_account_depleted(a, threshold))
        .unwrap_or(false);
    let active_pct = active.map(|a| a.last_primary_percentage).unwrap_or(0.0);

    let candidates: Vec<&AccountConfig> = accounts
        .iter()
        .filter(|a| a.enabled)
        .filter(|a| {
            if let Some(act) = active {
                a.id != act.id && !a.email.eq_ignore_ascii_case(&act.email)
            } else if let Some(id) = active_id {
                a.id != id && !a.email.eq_ignore_ascii_case(id)
            } else {
                true
            }
        })
        .filter(|a| !a.needs_relogin())
        .filter(|a| {
            a.last_error
                .as_deref()
                .map(|s| s.trim().is_empty())
                .unwrap_or(true)
        })
        .filter(|a| !is_account_depleted(a, threshold))
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Usable candidates MUST have quota strictly above threshold.
    // In addition:
    // - If active is depleted, any non-depleted candidate is ready.
    // - If business_priority is ON and active is non-business, any non-depleted business candidate is ready (preemption).
    // - Otherwise, candidate must have strictly higher quota than active.
    let mut ready_candidates: Vec<&AccountConfig> = candidates
        .into_iter()
        .filter(|a| {
            if active_depleted {
                return true;
            }
            if business_priority && active_is_non_business && a.is_business() {
                return true;
            }
            a.last_primary_percentage > active_pct
        })
        .collect();

    if ready_candidates.is_empty() {
        return None;
    }

    // When preempting a non-depleted non-business account in business_priority mode,
    // we MUST only switch to business accounts.
    if !active_depleted && business_priority && active_is_non_business {
        ready_candidates.retain(|a| a.is_business());
        if ready_candidates.is_empty() {
            return None;
        }
    }

    if business_only {
        ready_candidates.retain(|a| a.is_business());
        if ready_candidates.is_empty() {
            return None;
        }
    }

    let mut sorted = ready_candidates;
    sorted.sort_by(|a, b| {
        if business_priority {
            let biz_cmp = b.is_business().cmp(&a.is_business());
            if biz_cmp != std::cmp::Ordering::Equal {
                return biz_cmp;
            }
        }

        // Preference is given first to accounts with more resets (credits)
        let cred_a = a.last_credits.unwrap_or(0);
        let cred_b = b.last_credits.unwrap_or(0);
        let cred_cmp = cred_b.cmp(&cred_a);
        if cred_cmp != std::cmp::Ordering::Equal {
            return cred_cmp;
        }

        if strategy == "highest-quota" {
            b.last_primary_percentage
                .partial_cmp(&a.last_primary_percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.priority.cmp(&b.priority))
        } else {
            // Reset-First (default): pick the one that will reset soonest, or higher quota
            let a_reset = a.last_reset_after_seconds.unwrap_or(i64::MAX);
            let b_reset = b.last_reset_after_seconds.unwrap_or(i64::MAX);
            a_reset
                .cmp(&b_reset)
                .then_with(|| {
                    b.last_primary_percentage
                        .partial_cmp(&a.last_primary_percentage)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
                .then_with(|| a.priority.cmp(&b.priority))
        }
    });
    Some(sorted[0].id.clone())
}

#[cfg(test)]
#[path = "strategy.test.rs"]
mod tests;
