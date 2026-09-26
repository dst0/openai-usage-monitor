use crate::models::{AccountConfig, AccountsFile};
use crate::quota::ResetCreditConsumeOutcome;
use crate::storage::{load_accounts, update_accounts_atomically};

#[path = "manual_reset_attempt.rs"]
mod manual_reset_attempt;
#[path = "manual_reset_attempt_store.rs"]
mod manual_reset_attempt_store;

use manual_reset_attempt::ManualResetAttempt;
use manual_reset_attempt_store::ManualResetAttemptStore;

pub(crate) fn unresolved_manual_reset() -> Result<bool, String> {
    Ok(ManualResetAttemptStore::load()?.is_some_and(|attempt| attempt.is_unresolved()))
}

/// Consumes an available rate-limit reset credit for the specified account, restoring its quota.
pub fn reset_account(account_id: &str) -> Result<(), String> {
    let _operation = crate::recovery::operation_lock()?;
    let (name, is_active) = reset_account_transaction_with(
        account_id,
        crate::daemon::sync_active_tokens,
        crate::quota::consume_rate_limit_reset_credit,
    )?;

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

fn reset_account_transaction_with<S, F>(
    query: &str,
    synchronize: S,
    consume_fn: F,
) -> Result<(String, bool), String>
where
    S: FnOnce(&mut AccountsFile) -> Result<bool, String>,
    F: FnOnce(&AccountConfig, &str) -> crate::quota::ResetCreditConsumeOutcome,
{
    if ManualResetAttemptStore::load()?.is_some_and(|attempt| attempt.is_unresolved()) {
        return Err(
            "A previous manual reset is unresolved; reconcile its recorded attempt before requesting another credit"
                .into(),
        );
    }
    let mut file = load_accounts()?;
    synchronize(&mut file)?;
    let account_index = reset_target_index(&file, query)?;
    let target = file.accounts[account_index].clone();
    if crate::auto_reset::unresolved_auto_reset_for(&target)? {
        return Err(
            "An automatic reset for this account and weekly window is unresolved; reconcile it before requesting another credit"
                .into(),
        );
    }
    let available_credits = target.last_credits.unwrap_or(0);
    if available_credits == 0 {
        return Err(format!(
            "Account '{}' has no reset credits available",
            target.display_name()
        ));
    }
    let idempotency_key = crate::auto_reset::new_idempotency_key()?;
    let mut attempt = ManualResetAttempt::pending(
        target.id.clone(),
        available_credits,
        idempotency_key.clone(),
    );
    // Persist before dispatch. A crash or unknown response must not permit a
    // second invocation to mint a different idempotency key.
    ManualResetAttemptStore::write(&attempt)?;
    if !ManualResetAttemptStore::load()?
        .is_some_and(|saved| saved.matches_pending(&target.id, available_credits, &idempotency_key))
    {
        return Err("Manual reset attempt readback failed; no reset request was sent".into());
    }
    let outcome = consume_fn(&target, &idempotency_key);
    let local_result =
        apply_reset_outcome(&mut file, account_index, available_credits, outcome.clone());

    match outcome {
        ResetCreditConsumeOutcome::Applied => {
            local_result?;
            let consumed_credits = file.accounts[account_index].last_credits;
            match commit_consumed_credit(&target, consumed_credits) {
                Ok(result) => {
                    attempt.mark_resolved();
                    ManualResetAttemptStore::write(&attempt).map_err(|_| {
                        "Reset credit was consumed and cached, but attempt state is uncertain; do not retry"
                            .to_string()
                    })?;
                    Ok(result)
                }
                Err(_) => {
                    attempt.mark_applied_uncertain();
                    if ManualResetAttemptStore::write(&attempt).is_err() {
                        return Err("Reset credit was consumed, but cache and attempt state are uncertain; do not retry".into());
                    }
                    Err("Reset credit was consumed, but local cache is uncertain; do not retry automatically".into())
                }
            }
        }
        ResetCreditConsumeOutcome::Unknown(_) => {
            attempt.mark_unknown();
            ManualResetAttemptStore::write(&attempt).map_err(|_| {
                "Reset outcome and attempt state are uncertain; do not retry".to_string()
            })?;
            Err(
                "Reset credit outcome is uncertain; reconcile the recorded attempt before retrying"
                    .into(),
            )
        }
        ResetCreditConsumeOutcome::NotConsumed(_) | ResetCreditConsumeOutcome::Unavailable(_) => {
            attempt.mark_resolved();
            ManualResetAttemptStore::write(&attempt)?;
            local_result
        }
    }
}

fn commit_consumed_credit(
    target: &AccountConfig,
    consumed_credits: Option<u32>,
) -> Result<(String, bool), String> {
    // The remote credit is already consumed. Only update its local cache if
    // the exact target credentials and prior credit count still match.
    let latest = update_accounts_atomically(|registry| {
        let matching: Vec<_> = registry
            .accounts
            .iter()
            .enumerate()
            .filter_map(|(index, account)| (account.id == target.id).then_some(index))
            .collect();
        if matching.len() != 1 {
            return Err("Reset target changed while credit was consumed".into());
        }
        let current = &mut registry.accounts[matching[0]];
        if current.tokens != target.tokens
            || current.account_id != target.account_id
            || current.email != target.email
            || current.last_credits != target.last_credits
        {
            return Err("Reset account binding or credit cache changed".into());
        }
        current.last_credits = consumed_credits;
        Ok(())
    })?;
    let committed = latest
        .accounts
        .iter()
        .find(|account| account.id == target.id)
        .ok_or("Reset target disappeared after commit")?;
    Ok((
        committed.display_name().to_string(),
        latest.active_account_id.as_deref() == Some(target.id.as_str()),
    ))
}

#[cfg(test)]
pub(crate) fn reset_account_in_file<F>(
    file: &mut AccountsFile,
    query: &str,
    consume_fn: F,
) -> Result<(String, bool), String>
where
    F: FnOnce(&AccountConfig, &str) -> crate::quota::ResetCreditConsumeOutcome,
{
    let acc_idx = reset_target_index(file, query)?;
    let acc = &file.accounts[acc_idx];
    let available_credits = acc.last_credits.unwrap_or(0);
    if available_credits == 0 {
        return Err(format!(
            "Account '{}' has no reset credits available",
            acc.display_name()
        ));
    }

    let idempotency_key = crate::auto_reset::new_idempotency_key()?;
    let outcome = consume_fn(acc, &idempotency_key);
    apply_reset_outcome(file, acc_idx, available_credits, outcome)
}

fn apply_reset_outcome(
    file: &mut AccountsFile,
    acc_idx: usize,
    available_credits: u32,
    outcome: ResetCreditConsumeOutcome,
) -> Result<(String, bool), String> {
    match outcome {
        ResetCreditConsumeOutcome::Applied => {
            file.accounts[acc_idx].last_credits = Some(available_credits.saturating_sub(1));
            let name = file.accounts[acc_idx].display_name().to_string();
            let is_active = file.active_account_id.as_deref() == Some(&file.accounts[acc_idx].id);
            Ok((name, is_active))
        }
        ResetCreditConsumeOutcome::NotConsumed(reason) => {
            Err(format!("Reset credit was not consumed: {reason}"))
        }
        ResetCreditConsumeOutcome::Unavailable(reason) => {
            Err(format!("Reset credit service is unavailable: {reason}"))
        }
        ResetCreditConsumeOutcome::Unknown(reason) => {
            Err(format!("Reset credit outcome uncertain: {reason}"))
        }
    }
}

fn reset_target_index(file: &AccountsFile, query: &str) -> Result<usize, String> {
    let q = query.trim();
    if q.is_empty() {
        return Err("Account identifier cannot be empty".to_string());
    }

    let index = if q.eq_ignore_ascii_case("desktop-app")
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
    Ok(index)
}

#[cfg(test)]
#[path = "account_reset_service.test.rs"]
mod tests;
