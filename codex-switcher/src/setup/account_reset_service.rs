use crate::models::AccountConfig;
use crate::storage::{load_accounts, save_accounts};

/// Consumes an available rate-limit reset credit for the specified account, restoring its quota.
pub fn reset_account(account_id: &str) -> Result<(), String> {
    let mut file = load_accounts()?;
    // Sync any live credentials from auth.json
    let _ = crate::daemon::sync_active_tokens(&mut file);

    let (name, is_active) = reset_account_in_file(
        &mut file,
        account_id,
        crate::quota::consume_rate_limit_reset_credit,
    )?;
    save_accounts(&file)?;

    // Always refresh quotas and status cache so all accounts reflect the update
    let _ = crate::daemon::refresh_quotas_and_status();

    // If the reset was applied to the active account, recover any blocked tasks
    if is_active {
        let blocked = crate::switcher::detect_recent_quota_blocked_user_threads();
        if !blocked.is_empty() {
            let _ = crate::recovery::recover_threads(
                &blocked,
                crate::recovery::RecoveryMode::DiscoveredOnly,
            );
        }
    }

    println!(
        "✅ Successfully reset quota for account '{}'! 1 reset credit consumed.",
        name
    );
    Ok(())
}

pub(crate) fn reset_account_in_file<F>(
    file: &mut crate::models::AccountsFile,
    query: &str,
    consume_fn: F,
) -> Result<(String, bool), String>
where
    F: FnOnce(&AccountConfig, &str) -> crate::quota::ResetCreditConsumeOutcome,
{
    let q = query.trim();
    if q.is_empty() {
        return Err("Account identifier cannot be empty".to_string());
    }

    let acc_idx = if q.eq_ignore_ascii_case("desktop-app")
        || q.eq_ignore_ascii_case("active")
        || q.eq_ignore_ascii_case("current")
    {
        if let Some(active_id) = &file.active_account_id {
            crate::switcher::resolve_target_account_idx(&file.accounts, active_id)?
        } else if !file.accounts.is_empty() {
            0
        } else {
            return Err("No accounts configured".to_string());
        }
    } else {
        crate::switcher::resolve_target_account_idx(&file.accounts, q)?
    };

    let acc = &file.accounts[acc_idx];
    let available_credits = acc.last_credits.unwrap_or(0);
    if available_credits <= 0 {
        return Err(format!(
            "Account '{}' has no reset credits available",
            acc.display_name()
        ));
    }

    let idempotency_key = crate::auto_reset::new_idempotency_key()?;
    let outcome = consume_fn(acc, &idempotency_key);

    match outcome {
        crate::quota::ResetCreditConsumeOutcome::Applied => {
            file.accounts[acc_idx].last_credits = Some(available_credits.saturating_sub(1));
            // Only attempt cache refresh in real runtime (ignore failure in offline / test)
            let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                crate::quota::update_account_quota_cache(&mut file.accounts[acc_idx]);
            }));
            let name = file.accounts[acc_idx].display_name().to_string();
            let is_active = file.active_account_id.as_deref() == Some(&file.accounts[acc_idx].id);
            Ok((name, is_active))
        }
        crate::quota::ResetCreditConsumeOutcome::NotConsumed(reason) => {
            Err(format!("Reset credit was not consumed: {reason}"))
        }
        crate::quota::ResetCreditConsumeOutcome::Unavailable(reason) => {
            Err(format!("Reset credit service is unavailable: {reason}"))
        }
        crate::quota::ResetCreditConsumeOutcome::Unknown(reason) => {
            Err(format!("Reset credit outcome uncertain: {reason}"))
        }
    }
}
