use crate::models::{AccountConfig, AuthTokens};

/// Named quota and identity inputs for a distribution test account; fields left
/// at their defaults mean no name, 0% sprint quota, no credits, and no error.
/// `id`, `email`, and `plan` are required and `build` rejects them when empty.
#[derive(Debug, Clone, Default)]
pub struct TestAccountSpec<'a> {
    pub id: &'a str,
    pub name: Option<&'a str>,
    pub email: &'a str,
    pub plan: &'a str,
    pub sprint_pct: f64,
    pub weekly_pct: Option<f64>,
    pub credits: u32,
    pub reset_after: Option<i64>,
    pub error: Option<&'a str>,
}

impl TestAccountSpec<'_> {
    pub fn build(self) -> AccountConfig {
        assert!(
            !self.id.is_empty() && !self.email.is_empty() && !self.plan.is_empty(),
            "TestAccountSpec requires id, email, and plan"
        );
        let id = self.id;
        AccountConfig {
            id: id.to_string(),
            name: self.name.map(ToString::to_string),
            email: self.email.to_string(),
            plan_type: self.plan.to_string(),
            account_id: id.to_string(),
            tokens: AuthTokens {
                access_token: format!("tok_{id}"),
                refresh_token: Some(format!("rt_{id}")),
                id_token: None,
                account_id: Some(id.to_string()),
            },
            enabled: true,
            priority: 0,
            last_primary_percentage: self.sprint_pct,
            last_reset_time: None,
            last_reset_after_seconds: self.reset_after,
            last_weekly_percentage: self.weekly_pct,
            last_weekly_reset_time: None,
            last_weekly_reset_after_seconds: None,
            last_credits: Some(self.credits),
            last_error: self.error.map(ToString::to_string),
            last_checked: None,
            plan_multiplier: None,
            multiplier_is_manual: None,
            last_multiplier_checked: None,
            organization_name: None,
        }
    }
}

#[cfg(test)]
#[path = "test_account_spec.test.rs"]
mod tests;
