use super::automatic_distribution_service::AutomaticDistributionService;
use super::automatic_distribution_source::AutomaticDistributionSource;
use super::daemon_account_sync_service::DaemonAccountSyncService;
use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_executor::DistributionExecutor;
use super::distribution_outcome::DistributionOutcome;
use super::distribution_outcome::DistributionStatus;
use super::log_redaction_service::LogRedactionService;
use crate::models::{AccountStatusEntry, AccountsFile, StatusFile};
use crate::quota::update_account_quota_cache;
use crate::storage::{
    load_accounts, read_active_auth_json, save_accounts, write_active_auth_json, write_status_file,
};
use chrono::Utc;

pub struct DaemonTickService;

impl DaemonTickService {
    pub fn run(auto_switch: bool) -> Result<(), String> {
        let mut accounts_file = load_accounts().unwrap_or_default();
        let _ = DaemonAccountSyncService::sync_active_tokens(&mut accounts_file);
        if accounts_file.accounts.is_empty() {
            return Err("No accounts configured to monitor".to_string());
        }

        let current_active_id = accounts_file
            .active_account_id
            .clone()
            .unwrap_or_else(|| accounts_file.accounts[0].id.clone());
        accounts_file.active_account_id = Some(current_active_id.clone());
        refresh_quota_caches(&mut accounts_file, &current_active_id);
        DaemonAccountSyncService::cross_pollinate_organization_names(&mut accounts_file.accounts);

        let mut fresh = load_accounts().unwrap_or_default();
        merge_quota_caches(&accounts_file, &mut fresh);
        let _ = save_accounts(&fresh);
        accounts_file.settings = fresh.settings;

        let active = accounts_file
            .accounts
            .iter()
            .find(|account| account.id == current_active_id);
        let mut status = build_status(&accounts_file, active, &current_active_id);
        write_status_file(&status)?;

        if auto_switch {
            if let Some(account) = active {
                crate::logger::log(
                    "INFO",
                    "AUDIT",
                    &format!(
                        "Active account_ref={} sprint={:.1}% weekly={:.1}% credits={}",
                        LogRedactionService::sanitize_field("account_id", &account.id),
                        account.last_primary_percentage,
                        account.last_weekly_percentage.unwrap_or(100.0),
                        account.last_credits.unwrap_or(0)
                    ),
                );
            }
        }

        let weekly_reset_suppressed =
            handle_weekly_reset(auto_switch, &accounts_file, active, &mut status)?;
        if auto_switch {
            coordinate_automatic_distribution(&accounts_file, weekly_reset_suppressed)?;
        }
        Ok(())
    }
}

fn refresh_quota_caches(accounts_file: &mut AccountsFile, current_active_id: &str) {
    for account in &mut accounts_file.accounts {
        let is_active = account.id == current_active_id;
        if !account.enabled {
            continue;
        }
        let previous_tokens = account.tokens.clone();
        update_account_quota_cache(account);
        if is_active && account.needs_relogin() {
            if let Ok(fresh_auth) = read_active_auth_json() {
                if let Some(fresh_tokens) = fresh_auth.tokens {
                    if fresh_tokens != account.tokens {
                        account.tokens = fresh_tokens;
                        update_account_quota_cache(account);
                    }
                }
            }
        }
        if is_active && account.tokens != previous_tokens {
            if let Ok(mut current_auth) = read_active_auth_json() {
                current_auth.tokens = Some(account.tokens.clone());
                current_auth.last_refresh = Some(Utc::now().to_rfc3339());
                let _ = write_active_auth_json(&current_auth);
            }
        }
    }
}

fn merge_quota_caches(updated: &AccountsFile, fresh: &mut AccountsFile) {
    for updated_account in &updated.accounts {
        if let Some(account) = fresh
            .accounts
            .iter_mut()
            .find(|account| account.id == updated_account.id)
        {
            account.last_primary_percentage = updated_account.last_primary_percentage;
            account.last_reset_time = updated_account.last_reset_time.clone();
            account.last_reset_after_seconds = updated_account.last_reset_after_seconds;
            account.last_weekly_percentage = updated_account.last_weekly_percentage;
            account.last_weekly_reset_time = updated_account.last_weekly_reset_time.clone();
            account.last_weekly_reset_after_seconds =
                updated_account.last_weekly_reset_after_seconds;
            account.last_credits = updated_account.last_credits;
            account.last_error = updated_account.last_error.clone();
            account.last_checked = updated_account.last_checked.clone();
            account.tokens = updated_account.tokens.clone();
            account.organization_name = updated_account.organization_name.clone();
            if updated_account.multiplier_is_manual != Some(true) {
                account.plan_multiplier = updated_account.plan_multiplier;
                account.last_multiplier_checked = updated_account.last_multiplier_checked.clone();
            }
        }
    }
}

fn build_status(
    accounts_file: &AccountsFile,
    active: Option<&crate::models::AccountConfig>,
    current_active_id: &str,
) -> StatusFile {
    let auto_reset_status = crate::auto_reset::status_for_active(&accounts_file.settings, active);
    StatusFile {
        timestamp: Utc::now().to_rfc3339(),
        active_account_id: Some(current_active_id.to_string()),
        active_email: active.map(|account| account.email.clone()),
        active_plan: active.map(|account| account.plan_type.clone()),
        five_hour_percentage: active
            .map(|account| account.last_primary_percentage)
            .unwrap_or(100.0),
        weekly_percentage: active.and_then(|account| account.last_weekly_percentage),
        weekly_reset_time: active.and_then(|account| account.last_weekly_reset_time.clone()),
        weekly_reset_after_seconds: active
            .and_then(|account| account.last_weekly_reset_after_seconds),
        reset_time: active.and_then(|account| account.last_reset_time.clone()),
        reset_after_seconds: active.and_then(|account| account.last_reset_after_seconds),
        credits: active.and_then(|account| account.last_credits).unwrap_or(0),
        auto_switch_enabled: accounts_file.settings.auto_switch_enabled,
        auto_switch_business_only: accounts_file.settings.auto_switch_business_only,
        auto_switch_business_priority: accounts_file.settings.auto_switch_business_priority,
        auto_reset_weekly_enabled: accounts_file.settings.auto_reset_weekly_enabled,
        auto_reset_weekly_min_remaining_seconds: accounts_file
            .settings
            .auto_reset_weekly_min_remaining_seconds,
        auto_reset_state: auto_reset_status.state,
        auto_reset_reason: auto_reset_status.reason,
        auto_reset_last_event_at: auto_reset_status.last_event_at,
        plan_multiplier: active
            .map(|account| account.effective_multiplier())
            .unwrap_or(1.0),
        accounts: accounts_file
            .accounts
            .iter()
            .map(|account| account_status_entry(account, account.id == current_active_id))
            .collect(),
    }
}

fn account_status_entry(
    account: &crate::models::AccountConfig,
    is_active: bool,
) -> AccountStatusEntry {
    AccountStatusEntry {
        id: account.id.clone(),
        name: account.name.clone(),
        email: account.email.clone(),
        plan_type: account.plan_type.clone(),
        is_active,
        five_hour_percentage: account.last_primary_percentage,
        weekly_percentage: account.last_weekly_percentage,
        weekly_reset_time: account.last_weekly_reset_time.clone(),
        weekly_reset_after_seconds: account.last_weekly_reset_after_seconds,
        reset_time: account.last_reset_time.clone(),
        reset_after_seconds: account.last_reset_after_seconds,
        credits: account.last_credits.unwrap_or(0),
        error: account.last_error.clone(),
        plan_multiplier: account.effective_multiplier(),
        organization_name: account.organization_name.clone(),
    }
}

fn handle_weekly_reset(
    auto_switch: bool,
    accounts_file: &AccountsFile,
    active: Option<&crate::models::AccountConfig>,
    status: &mut StatusFile,
) -> Result<bool, String> {
    if !auto_switch || !accounts_file.settings.auto_reset_weekly_enabled {
        return Ok(false);
    }
    let Some(active) = active else {
        return Ok(false);
    };
    let report = crate::auto_reset::maybe_consume_weekly_reset(&accounts_file.settings, active)
        .unwrap_or_else(|error| crate::auto_reset::AutoResetReport {
            status: crate::auto_reset::AutoResetStatus {
                state: "journal_error".into(),
                reason: Some(error),
                last_event_at: None,
            },
            suppress_auto_switch: true,
        });
    status.auto_reset_state = report.status.state;
    status.auto_reset_reason = report.status.reason;
    status.auto_reset_last_event_at = report.status.last_event_at;
    write_status_file(status)?;
    Ok(report.suppress_auto_switch)
}

fn coordinate_automatic_distribution(
    accounts_file: &AccountsFile,
    suppressed: bool,
) -> Result<(), String> {
    let coordinator = DistributionCoordinator::new();
    if let Some(outcome) =
        coordinate_automatic_distribution_with(&coordinator, accounts_file, suppressed)?
    {
        match outcome.status {
            DistributionStatus::Success => {
                LogRedactionService::print_background("✅ Automatic account distribution completed")
            }
            DistributionStatus::PartialSuccess => LogRedactionService::eprint_background(
                "⚠️ Automatic account distribution completed with incomplete recovery",
            ),
            DistributionStatus::DeferredCooldown | DistributionStatus::DeferredInFlight => {
                LogRedactionService::print_background("ℹ️ Automatic account distribution deferred")
            }
            DistributionStatus::NoActionNeeded => {}
            DistributionStatus::Failed => {
                LogRedactionService::eprint_background("❌ Automatic account distribution failed")
            }
        }
    }
    Ok(())
}

pub(crate) fn coordinate_automatic_distribution_with<E: DistributionExecutor>(
    executor: &E,
    accounts_file: &AccountsFile,
    suppressed: bool,
) -> Result<Option<DistributionOutcome>, String> {
    AutomaticDistributionService::new(executor).execute(
        AutomaticDistributionSource::Daemon,
        accounts_file,
        suppressed,
    )
}

#[cfg(test)]
#[path = "daemon_tick_service.test.rs"]
mod tests;
