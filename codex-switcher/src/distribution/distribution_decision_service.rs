use super::distribution_candidate::DistributionCandidate;
use super::distribution_plan::DistributionPlan;
use super::distribution_request::DistributionRequest;
use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountConfig, AccountsFile};
use crate::strategy::is_account_depleted;

#[derive(Default)]
pub struct DistributionDecisionService;

impl DistributionDecisionService {
    pub fn new() -> Self {
        Self
    }

    pub fn evaluate(
        &self,
        accounts_file: &AccountsFile,
        current_app_id: Option<&str>,
        current_cli_id: Option<&str>,
        is_desktop_running: bool,
        request: &DistributionRequest,
    ) -> DistributionPlan {
        let threshold = accounts_file.settings.switch_threshold_percent;
        let business_only = accounts_file.settings.auto_switch_business_only;
        let business_priority = accounts_file.settings.auto_switch_business_priority;

        let mut evaluated_candidates: Vec<DistributionCandidate> = Vec::new();
        let mut eligible_accounts: Vec<&AccountConfig> = Vec::new();

        for acc in &accounts_file.accounts {
            let label = LogRedactionService::sanitize_field("account_id", &acc.id);

            if !acc.enabled {
                evaluated_candidates.push(DistributionCandidate::ineligible(
                    acc.id.clone(),
                    label,
                    acc.plan_type.clone(),
                    acc.is_business(),
                    acc.last_primary_percentage,
                    acc.last_weekly_percentage,
                    acc.last_credits.unwrap_or(0),
                    acc.last_reset_after_seconds,
                    acc.priority,
                    "account_disabled",
                ));
                continue;
            }

            if acc.needs_relogin() {
                evaluated_candidates.push(DistributionCandidate::ineligible(
                    acc.id.clone(),
                    label,
                    acc.plan_type.clone(),
                    acc.is_business(),
                    acc.last_primary_percentage,
                    acc.last_weekly_percentage,
                    acc.last_credits.unwrap_or(0),
                    acc.last_reset_after_seconds,
                    acc.priority,
                    "needs_relogin",
                ));
                continue;
            }

            if acc
                .last_error
                .as_deref()
                .is_some_and(|error| !error.trim().is_empty())
            {
                evaluated_candidates.push(DistributionCandidate::ineligible(
                    acc.id.clone(),
                    label,
                    acc.plan_type.clone(),
                    acc.is_business(),
                    acc.last_primary_percentage,
                    acc.last_weekly_percentage,
                    acc.last_credits.unwrap_or(0),
                    acc.last_reset_after_seconds,
                    acc.priority,
                    "active_error",
                ));
                continue;
            }

            if acc.last_primary_percentage <= threshold {
                evaluated_candidates.push(DistributionCandidate::ineligible(
                    acc.id.clone(),
                    label,
                    acc.plan_type.clone(),
                    acc.is_business(),
                    acc.last_primary_percentage,
                    acc.last_weekly_percentage,
                    acc.last_credits.unwrap_or(0),
                    acc.last_reset_after_seconds,
                    acc.priority,
                    "depleted_5h",
                ));
                continue;
            }

            if let Some(weekly) = acc.last_weekly_percentage {
                if weekly <= threshold && acc.last_credits.unwrap_or(0) == 0 {
                    evaluated_candidates.push(DistributionCandidate::ineligible(
                        acc.id.clone(),
                        label,
                        acc.plan_type.clone(),
                        acc.is_business(),
                        acc.last_primary_percentage,
                        acc.last_weekly_percentage,
                        acc.last_credits.unwrap_or(0),
                        acc.last_reset_after_seconds,
                        acc.priority,
                        "weekly_exhausted_no_credits",
                    ));
                    continue;
                }
            }

            if business_only && !acc.is_business() {
                evaluated_candidates.push(DistributionCandidate::ineligible(
                    acc.id.clone(),
                    label,
                    acc.plan_type.clone(),
                    acc.is_business(),
                    acc.last_primary_percentage,
                    acc.last_weekly_percentage,
                    acc.last_credits.unwrap_or(0),
                    acc.last_reset_after_seconds,
                    acc.priority,
                    "business_only_filter",
                ));
                continue;
            }

            evaluated_candidates.push(DistributionCandidate::eligible(
                acc.id.clone(),
                label,
                acc.plan_type.clone(),
                acc.is_business(),
                acc.last_primary_percentage,
                acc.last_weekly_percentage,
                acc.last_credits.unwrap_or(0),
                acc.last_reset_after_seconds,
                acc.priority,
            ));
            eligible_accounts.push(acc);
        }

        eligible_accounts.sort_by(|a, b| {
            if business_priority {
                let biz_cmp = b.is_business().cmp(&a.is_business());
                if biz_cmp != std::cmp::Ordering::Equal {
                    return biz_cmp;
                }
            }
            let cred_a = a.last_credits.unwrap_or(0);
            let cred_b = b.last_credits.unwrap_or(0);
            if cred_b != cred_a {
                return cred_b.cmp(&cred_a);
            }
            let a_reset = a.last_reset_after_seconds.unwrap_or(i64::MAX);
            let b_reset = b.last_reset_after_seconds.unwrap_or(i64::MAX);
            let percentage = || {
                b.last_primary_percentage
                    .partial_cmp(&a.last_primary_percentage)
                    .unwrap_or(std::cmp::Ordering::Equal)
            };
            if accounts_file.settings.strategy == "highest-quota" {
                percentage()
                    .then_with(|| a_reset.cmp(&b_reset))
                    .then_with(|| a.priority.cmp(&b.priority))
            } else {
                a_reset
                    .cmp(&b_reset)
                    .then_with(percentage)
                    .then_with(|| a.priority.cmp(&b.priority))
            }
        });

        let canonical_current_app = current_app_id.and_then(|id| {
            crate::switcher::resolve_target_account_idx(&accounts_file.accounts, id)
                .ok()
                .map(|idx| accounts_file.accounts[idx].id.clone())
        });

        let canonical_current_cli = current_cli_id.and_then(|id| {
            crate::switcher::resolve_target_account_idx(&accounts_file.accounts, id)
                .ok()
                .map(|idx| accounts_file.accounts[idx].id.clone())
        });

        let current_app_val =
            canonical_current_app.or_else(|| current_app_id.map(ToString::to_string));
        let current_cli_val =
            canonical_current_cli.or_else(|| current_cli_id.map(ToString::to_string));

        if eligible_accounts.is_empty() {
            return DistributionPlan::no_action(
                current_app_val,
                current_cli_val,
                "no_viable_candidates",
                evaluated_candidates,
            );
        }

        let target_app = request
            .preferred_app_id
            .as_deref()
            .map(|id| {
                crate::switcher::resolve_target_account_idx(&accounts_file.accounts, id)
                    .map(|idx| accounts_file.accounts[idx].id.clone())
                    .unwrap_or_else(|_| id.to_string())
            })
            .or_else(|| eligible_accounts.first().map(|a| a.id.clone()));

        let target_cli = request
            .preferred_cli_id
            .as_deref()
            .map(|id| {
                crate::switcher::resolve_target_account_idx(&accounts_file.accounts, id)
                    .map(|idx| accounts_file.accounts[idx].id.clone())
                    .unwrap_or_else(|_| id.to_string())
            })
            .or_else(|| {
                if eligible_accounts.len() >= 2 {
                    Some(eligible_accounts[1].id.clone())
                } else {
                    eligible_accounts.first().map(|a| a.id.clone())
                }
            });

        let app_depleted = current_app_val
            .as_deref()
            .and_then(|id| {
                accounts_file
                    .accounts
                    .iter()
                    .find(|a| a.id.eq_ignore_ascii_case(id))
            })
            .map(|a| is_account_depleted(a, threshold))
            .unwrap_or(current_app_id.is_none());

        let cli_depleted = current_cli_val
            .as_deref()
            .and_then(|id| {
                accounts_file
                    .accounts
                    .iter()
                    .find(|a| a.id.eq_ignore_ascii_case(id))
            })
            .map(|a| is_account_depleted(a, threshold))
            .unwrap_or(current_cli_id.is_none());

        let app_matches = current_app_val.is_some()
            && current_app_val
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case(target_app.as_deref().unwrap_or("")))
                == Some(true);

        let cli_matches = current_cli_val.is_some()
            && current_cli_val
                .as_deref()
                .map(|s| s.eq_ignore_ascii_case(target_cli.as_deref().unwrap_or("")))
                == Some(true);

        let should_separate = eligible_accounts.len() >= 2
            && current_app_val.is_some()
            && current_cli_val.is_some()
            && current_app_val == current_cli_val;
        let app_switch_needed = is_desktop_running
            && request.allow_restart
            && (!app_matches || app_depleted || should_separate);
        let cli_switch_needed = !cli_matches || cli_depleted || should_separate;
        if !app_switch_needed && !cli_switch_needed && !request.force_restart {
            return DistributionPlan::no_action(
                current_app_val,
                current_cli_val,
                "already_optimal",
                evaluated_candidates,
            );
        }

        DistributionPlan {
            current_app_id: current_app_val.clone(),
            current_cli_id: current_cli_val,
            target_app_id: if is_desktop_running && !request.allow_restart {
                current_app_val.clone()
            } else {
                target_app
            },
            target_cli_id: target_cli,
            app_switch_needed,
            cli_switch_needed,
            restart_required: app_switch_needed,
            decision_reason: request.reason.clone(),
            evaluated_candidates,
        }
    }
}
