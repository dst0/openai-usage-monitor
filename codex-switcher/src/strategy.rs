use crate::models::AccountConfig;

pub fn needs_switch(account: &AccountConfig, threshold: f64) -> bool {
    account.last_primary_percentage <= threshold
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
        .filter(|a| a.last_error.is_none())
        .collect();

    if candidates.is_empty() {
        return None;
    }

    // Usable candidates MUST have quota strictly above threshold AND strictly greater than current active quota.
    // We NEVER switch to another exhausted account (0% quota) or disrupt a Pro account without a usable alternative.
    let mut ready_candidates: Vec<&AccountConfig> = candidates
        .into_iter()
        .filter(|a| a.last_primary_percentage > threshold && a.last_primary_percentage > active_pct)
        .collect();

    if ready_candidates.is_empty() {
        return None;
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
mod tests {
    use super::*;
    use crate::models::AuthTokens;

    fn make_acc(id: &str, pct: f64, reset: i64) -> AccountConfig {
        make_acc_with_credits_and_plan(id, pct, reset, None, "team")
    }

    fn make_acc_with_credits_and_plan(
        id: &str,
        pct: f64,
        reset: i64,
        credits: Option<u32>,
        plan_type: &str,
    ) -> AccountConfig {
        AccountConfig {
            id: id.to_string(),
            name: None,
            email: format!("{}@example.com", id),
            plan_type: plan_type.to_string(),
            account_id: id.to_string(),
            tokens: AuthTokens {
                access_token: "tok".to_string(),
                refresh_token: None,
                id_token: None,
                account_id: None,
            },
            enabled: true,
            priority: 0,
            last_primary_percentage: pct,
            last_reset_time: None,
            last_reset_after_seconds: Some(reset),
            last_weekly_percentage: None,
            last_credits: credits,
            last_error: None,
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
        }
    }

    #[test]
    fn test_select_best_switch_ready() {
        let accounts = vec![
            make_acc("acc-1", 0.0, 3600),
            make_acc("acc-2", 80.0, 7200),
            make_acc("acc-3", 95.0, 1800),
        ];

        // acc-3 has quota and resets in 1800s vs acc-2 in 7200s
        let chosen = select_best_switch(Some("acc-1"), &accounts, 0.0, "reset-first", false, false);
        assert_eq!(chosen, Some("acc-3".to_string()));
    }

    #[test]
    fn test_single_account_never_switches() {
        let accounts = vec![make_acc("acc-1", 0.0, 3600)];
        assert_eq!(select_best_switch(Some("acc-1"), &accounts, 10.0, "reset-first", false, false), None);
        assert_eq!(select_best_switch(None, &accounts, 10.0, "reset-first", false, false), None);
    }

    #[test]
    fn test_all_exhausted_candidates_never_switches() {
        let mut pro_acc = make_acc("pro-user", 0.0, 3600);
        pro_acc.plan_type = "pro".to_string();
        let accounts = vec![
            pro_acc,
            make_acc("reserve-1", 0.0, 1800),
            make_acc("reserve-2", 5.0, 7200),
        ];

        // With threshold = 10.0, neither reserve has quota > threshold (0% and 5%)
        // Crucial test: MUST return None, NEVER switch or exit pro-user!
        let chosen = select_best_switch(Some("pro-user"), &accounts, 10.0, "reset-first", false, false);
        assert_eq!(chosen, None);
    }

    #[test]
    fn test_candidate_must_have_greater_quota_than_active() {
        let accounts = vec![
            make_acc("acc-1", 50.0, 3600),
            make_acc("acc-2", 30.0, 1800),
        ];
        // acc-2 has 30% > threshold (10%), but less than active (50%) -> should NOT switch
        assert_eq!(select_best_switch(Some("acc-1"), &accounts, 10.0, "reset-first", false, false), None);
    }

    #[test]
    fn test_preference_for_more_resets_credits() {
        let accounts = vec![
            make_acc_with_credits_and_plan("acc-active", 0.0, 3600, Some(0), "team"),
            // acc-1 has 90% quota and resets in 1000s, but 0 credits
            make_acc_with_credits_and_plan("acc-1", 90.0, 1000, Some(0), "team"),
            // acc-2 has 80% quota and resets in 2000s, but 3 credits (resets)
            make_acc_with_credits_and_plan("acc-2", 80.0, 2000, Some(3), "team"),
        ];

        // acc-2 has MORE resets (credits: 3 vs 0), so it must be selected first!
        let chosen = select_best_switch(Some("acc-active"), &accounts, 0.0, "reset-first", false, false);
        assert_eq!(chosen, Some("acc-2".to_string()));
    }

    #[test]
    fn test_business_only_mode() {
        let accounts = vec![
            make_acc_with_credits_and_plan("biz-active", 0.0, 3600, Some(0), "team"),
            // Personal account with 100% quota and 5 credits
            make_acc_with_credits_and_plan("personal-1", 100.0, 1000, Some(5), "plus"),
            // Business account with 60% quota and 1 credit
            make_acc_with_credits_and_plan("biz-2", 60.0, 2000, Some(1), "business"),
        ];

        // In business_only mode: personal-1 is ignored, biz-2 is selected
        let chosen = select_best_switch(Some("biz-active"), &accounts, 0.0, "reset-first", true, false);
        assert_eq!(chosen, Some("biz-2".to_string()));

        // If all business accounts are exhausted, business_only returns None even if personal has quota
        let exhausted_biz = vec![
            make_acc_with_credits_and_plan("biz-active", 0.0, 3600, Some(0), "team"),
            make_acc_with_credits_and_plan("personal-1", 100.0, 1000, Some(5), "plus"),
            make_acc_with_credits_and_plan("biz-2", 0.0, 2000, Some(1), "business"),
        ];
        let chosen_exhausted = select_best_switch(Some("biz-active"), &exhausted_biz, 0.0, "reset-first", true, false);
        assert_eq!(chosen_exhausted, None);
    }

    #[test]
    fn test_business_priority_mode() {
        let accounts = vec![
            make_acc_with_credits_and_plan("active-acc", 0.0, 3600, Some(0), "plus"),
            // Personal account with 100% quota and 5 credits
            make_acc_with_credits_and_plan("personal-1", 100.0, 1000, Some(5), "plus"),
            // Business account with 70% quota and 1 credit
            make_acc_with_credits_and_plan("biz-1", 70.0, 2000, Some(1), "team"),
            // Business account with 80% quota and 2 credits
            make_acc_with_credits_and_plan("biz-2", 80.0, 2000, Some(2), "business"),
        ];

        // In business_priority: business accounts are used FIRST.
        // Between biz-1 and biz-2: biz-2 has more credits (2 > 1), so biz-2 is selected!
        let chosen = select_best_switch(Some("active-acc"), &accounts, 0.0, "reset-first", false, true);
        assert_eq!(chosen, Some("biz-2".to_string()));

        // When all business accounts are exhausted, fallback to personal-1:
        let exhausted_biz_accounts = vec![
            make_acc_with_credits_and_plan("active-acc", 0.0, 3600, Some(0), "team"),
            make_acc_with_credits_and_plan("biz-1", 0.0, 2000, Some(1), "team"),
            make_acc_with_credits_and_plan("biz-2", 0.0, 2000, Some(2), "business"),
            make_acc_with_credits_and_plan("personal-1", 90.0, 1000, Some(3), "plus"),
        ];
        let fallback_chosen = select_best_switch(Some("active-acc"), &exhausted_biz_accounts, 0.0, "reset-first", false, true);
        assert_eq!(fallback_chosen, Some("personal-1".to_string()));
    }
}
