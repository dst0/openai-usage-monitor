use crate::models::AccountConfig;

pub fn needs_switch(account: &AccountConfig, threshold: f64) -> bool {
    account.last_primary_percentage <= threshold
}

pub fn select_best_switch(
    active_id: Option<&str>,
    accounts: &[AccountConfig],
    threshold: f64,
    strategy: &str,
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
    let ready_candidates: Vec<&AccountConfig> = candidates
        .into_iter()
        .filter(|a| a.last_primary_percentage > threshold && a.last_primary_percentage > active_pct)
        .collect();

    if ready_candidates.is_empty() {
        return None;
    }

    let mut sorted = ready_candidates;
    if strategy == "highest-quota" {
        sorted.sort_by(|a, b| {
            b.last_primary_percentage
                .partial_cmp(&a.last_primary_percentage)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| a.priority.cmp(&b.priority))
        });
    } else {
        // Reset-First (default): pick the one that will reset soonest, or higher quota
        sorted.sort_by(|a, b| {
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
        });
    }
    Some(sorted[0].id.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::AuthTokens;

    fn make_acc(id: &str, pct: f64, reset: i64) -> AccountConfig {
        AccountConfig {
            id: id.to_string(),
            name: None,
            email: format!("{}@example.com", id),
            plan_type: "team".to_string(),
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
            last_credits: None,
            last_error: None,
            last_checked: None,
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
        let chosen = select_best_switch(Some("acc-1"), &accounts, 0.0, "reset-first");
        assert_eq!(chosen, Some("acc-3".to_string()));
    }

    #[test]
    fn test_single_account_never_switches() {
        let accounts = vec![make_acc("acc-1", 0.0, 3600)];
        assert_eq!(select_best_switch(Some("acc-1"), &accounts, 10.0, "reset-first"), None);
        assert_eq!(select_best_switch(None, &accounts, 10.0, "reset-first"), None);
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
        let chosen = select_best_switch(Some("pro-user"), &accounts, 10.0, "reset-first");
        assert_eq!(chosen, None);
    }

    #[test]
    fn test_candidate_must_have_greater_quota_than_active() {
        let accounts = vec![
            make_acc("acc-1", 50.0, 3600),
            make_acc("acc-2", 30.0, 1800),
        ];
        // acc-2 has 30% > threshold (10%), but less than active (50%) -> should NOT switch
        assert_eq!(select_best_switch(Some("acc-1"), &accounts, 10.0, "reset-first"), None);
    }
}
