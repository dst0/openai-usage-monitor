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
            last_weekly_reset_time: None,
            last_weekly_reset_after_seconds: None,
            last_credits: credits,
            last_error: None,
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
            organization_name: None,
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
        assert_eq!(
            select_best_switch(Some("acc-1"), &accounts, 10.0, "reset-first", false, false),
            None
        );
        assert_eq!(
            select_best_switch(None, &accounts, 10.0, "reset-first", false, false),
            None
        );
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
        let chosen = select_best_switch(
            Some("pro-user"),
            &accounts,
            10.0,
            "reset-first",
            false,
            false,
        );
        assert_eq!(chosen, None);
    }

    #[test]
    fn test_candidate_must_have_greater_quota_than_active() {
        let accounts = vec![make_acc("acc-1", 50.0, 3600), make_acc("acc-2", 30.0, 1800)];
        // acc-2 has 30% > threshold (10%), but less than active (50%) -> should NOT switch
        assert_eq!(
            select_best_switch(Some("acc-1"), &accounts, 10.0, "reset-first", false, false),
            None
        );
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
        let chosen = select_best_switch(
            Some("acc-active"),
            &accounts,
            0.0,
            "reset-first",
            false,
            false,
        );
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
        let chosen = select_best_switch(
            Some("biz-active"),
            &accounts,
            0.0,
            "reset-first",
            true,
            false,
        );
        assert_eq!(chosen, Some("biz-2".to_string()));

        // If all business accounts are exhausted, business_only returns None even if personal has quota
        let exhausted_biz = vec![
            make_acc_with_credits_and_plan("biz-active", 0.0, 3600, Some(0), "team"),
            make_acc_with_credits_and_plan("personal-1", 100.0, 1000, Some(5), "plus"),
            make_acc_with_credits_and_plan("biz-2", 0.0, 2000, Some(1), "business"),
        ];
        let chosen_exhausted = select_best_switch(
            Some("biz-active"),
            &exhausted_biz,
            0.0,
            "reset-first",
            true,
            false,
        );
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
        let chosen = select_best_switch(
            Some("active-acc"),
            &accounts,
            0.0,
            "reset-first",
            false,
            true,
        );
        assert_eq!(chosen, Some("biz-2".to_string()));

        // When all business accounts are exhausted, fallback to personal-1:
        let exhausted_biz_accounts = vec![
            make_acc_with_credits_and_plan("active-acc", 0.0, 3600, Some(0), "team"),
            make_acc_with_credits_and_plan("biz-1", 0.0, 2000, Some(1), "team"),
            make_acc_with_credits_and_plan("biz-2", 0.0, 2000, Some(2), "business"),
            make_acc_with_credits_and_plan("personal-1", 90.0, 1000, Some(3), "plus"),
        ];
        let fallback_chosen = select_best_switch(
            Some("active-acc"),
            &exhausted_biz_accounts,
            0.0,
            "reset-first",
            false,
            true,
        );
        assert_eq!(fallback_chosen, Some("personal-1".to_string()));
    }

    #[test]
    fn test_needs_switch_and_preemption_when_business_quota_restores() {
        let active_pro = make_acc_with_credits_and_plan("pro-active", 95.0, 7200, Some(10), "pro");
        let exhausted_biz = make_acc_with_credits_and_plan("biz-1", 0.0, 3600, Some(2), "team");
        let restored_biz =
            make_acc_with_credits_and_plan("biz-restored", 80.0, 1800, Some(4), "business");

        // 1. While business account is exhausted, active Pro (95%) does NOT need switch
        let accounts_exhausted = vec![active_pro.clone(), exhausted_biz.clone()];
        assert!(!needs_switch(&active_pro, 0.0, true, &accounts_exhausted));
        assert_eq!(
            select_best_switch(
                Some("pro-active"),
                &accounts_exhausted,
                0.0,
                "reset-first",
                false,
                true
            ),
            None
        );

        // 2. When business account quota restores (80%), active Pro MUST trigger needs_switch
        let accounts_restored = vec![active_pro.clone(), restored_biz.clone()];
        assert!(needs_switch(&active_pro, 0.0, true, &accounts_restored));

        // 3. select_best_switch MUST preempt Pro (95%) in favor of restored business account (80%)!
        let chosen = select_best_switch(
            Some("pro-active"),
            &accounts_restored,
            0.0,
            "reset-first",
            false,
            true,
        );
        assert_eq!(chosen, Some("biz-restored".to_string()));

        // 4. Once on business account, even if another business account has more quota, needs_switch is false (no switch triggered)
        let accounts_two_biz = vec![
            make_acc_with_credits_and_plan("biz-current", 60.0, 3600, Some(1), "team"),
            make_acc_with_credits_and_plan("biz-other", 100.0, 1800, Some(5), "business"),
        ];
        assert!(!needs_switch(
            &accounts_two_biz[0],
            0.0,
            true,
            &accounts_two_biz
        ));

        // When biz-current exhausts its quota (0%), needs_switch triggers and switches to biz-other
        let mut biz_current_exhausted = accounts_two_biz.clone();
        biz_current_exhausted[0].last_primary_percentage = 0.0;
        assert!(needs_switch(
            &biz_current_exhausted[0],
            0.0,
            true,
            &biz_current_exhausted
        ));
        assert_eq!(
            select_best_switch(
                Some("biz-current"),
                &biz_current_exhausted,
                0.0,
                "reset-first",
                false,
                true
            ),
            Some("biz-other".to_string())
        );
    }

    #[test]
    fn test_weekly_quota_exhaustion_with_zero_credits_triggers_switch() {
        let mut active =
            make_acc_with_credits_and_plan("active-team", 64.0, 18000, Some(0), "team");
        active.last_weekly_percentage = Some(0.0);
        let candidate =
            make_acc_with_credits_and_plan("reserve-plus", 100.0, 18000, Some(2), "plus");
        let accounts = vec![active.clone(), candidate.clone()];

        // Active has 64% 5h sprint, but 0% weekly and 0 credits -> depleted!
        assert!(needs_switch(&active, 0.0, false, &accounts));
        let chosen = select_best_switch(
            Some("active-team"),
            &accounts,
            0.0,
            "reset-first",
            false,
            false,
        );
        assert_eq!(chosen, Some("reserve-plus".to_string()));
    }

    #[test]
    fn test_weekly_quota_zero_with_available_credits_is_not_depleted() {
        let mut active =
            make_acc_with_credits_and_plan("active-team", 64.0, 18000, Some(2), "team");
        active.last_weekly_percentage = Some(0.0);
        assert!(!is_account_depleted(&active, 0.0));
    }

    #[test]
    fn test_quota_error_triggers_switch() {
        let mut active =
            make_acc_with_credits_and_plan("active-team", 50.0, 18000, Some(0), "team");
        active.last_error = Some("429 Too Many Requests (usage_limit_exceeded)".to_string());
        let candidate =
            make_acc_with_credits_and_plan("reserve-plus", 100.0, 18000, Some(2), "plus");
        let accounts = vec![active.clone(), candidate.clone()];

        assert!(needs_switch(&active, 0.0, false, &accounts));
        let chosen = select_best_switch(
            Some("active-team"),
            &accounts,
            0.0,
            "reset-first",
            false,
            false,
        );
        assert_eq!(chosen, Some("reserve-plus".to_string()));
    }

    #[test]
    fn test_candidate_with_zero_weekly_quota_and_zero_credits_is_not_selected() {
        let active = make_acc_with_credits_and_plan("active-acc", 0.0, 18000, Some(0), "team");
        let mut depleted_candidate =
            make_acc_with_credits_and_plan("candidate-1", 80.0, 18000, Some(0), "team");
        depleted_candidate.last_weekly_percentage = Some(0.0);
        let healthy_candidate =
            make_acc_with_credits_and_plan("candidate-2", 40.0, 18000, Some(1), "team");

        let accounts = vec![active.clone(), depleted_candidate, healthy_candidate];
        let chosen = select_best_switch(
            Some("active-acc"),
            &accounts,
            0.0,
            "reset-first",
            false,
            false,
        );
        // candidate-1 has 80% sprint but 0% weekly and 0 credits, so candidate-2 (40%) MUST be chosen
        assert_eq!(chosen, Some("candidate-2".to_string()));
    }

    #[test]
    fn test_needs_relogin_detection() {
        let mut acc = make_acc_with_credits_and_plan("test", 100.0, 18000, Some(0), "team");
        assert!(!acc.needs_relogin());

        // Error keyword checks
        let error_cases = [
            "401 Unauthorized",
            "Session ended (logged out in app). Re-login required.",
            "User logged out",
            "unauthorized request",
            "token_revoked",
            "invalid_grant: refresh token expired",
            "refresh_token_invalidated",
            "relogin needed",
            "re-login required",
        ];
        for err in error_cases {
            acc.last_error = Some(err.to_string());
            assert!(acc.needs_relogin(), "Error '{}' must require relogin", err);
            assert!(
                is_account_depleted(&acc, 0.0),
                "Account with error '{}' must be depleted",
                err
            );
        }

        // Quota error must NOT require relogin
        acc.last_error = Some("429 Too Many Requests (Rate limit reached)".to_string());
        assert!(!acc.needs_relogin());

        // Empty access token requires relogin
        acc.last_error = None;
        acc.tokens.access_token = "   ".to_string();
        assert!(
            acc.needs_relogin(),
            "Empty access token must require relogin"
        );
    }

    #[test]
    fn test_active_account_needing_relogin_triggers_switch() {
        let mut active =
            make_acc_with_credits_and_plan("active-acc", 100.0, 18000, Some(2), "team");
        active.last_error = Some("401 Unauthorized (Session ended)".to_string());
        let candidate = make_acc_with_credits_and_plan("reserve-acc", 80.0, 18000, Some(0), "team");
        let accounts = vec![active.clone(), candidate.clone()];

        // Active account must be considered depleted and trigger switch
        assert!(is_account_depleted(&active, 0.0));
        assert!(needs_switch(&active, 0.0, false, &accounts));

        let chosen = select_best_switch(
            Some("active-acc"),
            &accounts,
            0.0,
            "reset-first",
            false,
            false,
        );
        assert_eq!(chosen, Some("reserve-acc".to_string()));
    }

    #[test]
    fn test_candidate_needing_relogin_is_never_selected() {
        let active = make_acc_with_credits_and_plan("active-acc", 0.0, 18000, Some(0), "team");
        let mut expired_candidate =
            make_acc_with_credits_and_plan("expired-cand", 100.0, 18000, Some(5), "team");
        expired_candidate.last_error = Some("Session ended (logged out in app)".to_string());
        let healthy_candidate =
            make_acc_with_credits_and_plan("healthy-cand", 40.0, 18000, Some(0), "team");

        let accounts = vec![active.clone(), expired_candidate, healthy_candidate];
        let chosen = select_best_switch(
            Some("active-acc"),
            &accounts,
            0.0,
            "reset-first",
            false,
            false,
        );
        // Even though expired_cand has 100% and 5 credits, healthy-cand (40%) MUST be chosen
        assert_eq!(chosen, Some("healthy-cand".to_string()));
    }
}
