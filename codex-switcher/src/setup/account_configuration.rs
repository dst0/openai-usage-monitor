use crate::storage::{load_accounts, save_accounts, sync_settings_to_status_file};

/// Renames an account's display nickname, or clears it if new_name is None.
/// Guarantees that nicknames are not duplicated across different accounts.
pub fn rename_account(query: &str, new_name: Option<&str>) -> Result<(), String> {
    rename_account_with(query, new_name, crate::daemon::refresh_quotas_and_status)
}

/// Renames an account, then runs `refresh_status` so the Menu Bar app shows the
/// new label. A failed refresh does not undo or fail the saved rename.
pub(crate) fn rename_account_with(
    query: &str,
    new_name: Option<&str>,
    refresh_status: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    let mut file = load_accounts()?;
    let idx = crate::switcher::resolve_target_account_idx(&file.accounts, query)?;

    let clean_new = new_name.map(str::trim).filter(|s| !s.is_empty());

    // Check for duplicate nicknames across other accounts
    if let Some(name) = clean_new {
        for (i, a) in file.accounts.iter().enumerate() {
            if i != idx {
                if let Some(existing_name) = &a.name {
                    if existing_name.trim().eq_ignore_ascii_case(name) {
                        return Err(format!(
                            "Nickname '{}' is already used by another account ({})",
                            name, a.email
                        ));
                    }
                }
            }
        }
    }

    let acc = &mut file.accounts[idx];
    acc.name = clean_new.map(str::to_string);
    let updated_id = acc.id.clone();
    let updated_name = acc.name.clone();

    save_accounts(&file)?;

    // Refresh quotas and status file so Menu Bar app updates immediately
    let _ = refresh_status();

    if let Some(name) = updated_name {
        println!("✅ Account '{}' renamed to '{}'", updated_id, name);
    } else {
        println!("✅ Cleared nickname for account '{}'", updated_id);
    }

    Ok(())
}

/// Updates the restart_app_on_switch setting in accounts.json.
pub fn set_config_restart_app_on_switch(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.restart_app_on_switch = enabled;
    save_accounts(&file)?;
    println!("✅ Setting updated: restart_app_on_switch = {}", enabled);
    Ok(())
}

/// Updates the preserve_window_bounds_on_restart setting in accounts.json.
pub fn set_config_preserve_window_bounds(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.preserve_window_bounds_on_restart = enabled;
    save_accounts(&file)?;
    println!(
        "✅ Setting updated: preserve_window_bounds_on_restart = {}",
        enabled
    );
    Ok(())
}

/// Updates the auto_switch_enabled setting in accounts.json.
pub fn set_config_auto_switch_enabled(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.auto_switch_enabled = enabled;
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!("✅ Setting updated: auto_switch_enabled = {}", enabled);
    Ok(())
}

/// Updates the auto_switch_business_only setting in accounts.json.
pub fn set_config_auto_switch_business_only(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.auto_switch_business_only = enabled;
    if enabled {
        file.settings.auto_switch_business_priority = false;
        file.settings.auto_switch_enabled = true;
    }
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!(
        "✅ Setting updated: auto_switch_business_only = {}",
        enabled
    );
    Ok(())
}

/// Updates the auto_switch_business_priority setting in accounts.json.
pub fn set_config_auto_switch_business_priority(enabled: bool) -> Result<(), String> {
    let mut file = load_accounts()?;
    file.settings.auto_switch_business_priority = enabled;
    if enabled {
        file.settings.auto_switch_business_only = false;
        file.settings.auto_switch_enabled = true;
    }
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!(
        "✅ Setting updated: auto_switch_business_priority = {}",
        enabled
    );
    Ok(())
}

/// Updates the opt-in weekly reset-credit policy. A threshold of zero means
/// that the policy may act whenever the weekly pool is exactly exhausted;
/// non-zero thresholds require that many seconds to remain before the normal
/// weekly reset. Keeping this as one atomic accounts.json write prevents the
/// daemon from observing a half-updated policy when the menu changes both
/// values together.
pub fn set_config_auto_reset_weekly(
    enabled: bool,
    min_remaining_seconds: u64,
) -> Result<(), String> {
    const MAX_REMAINING_SECONDS: u64 = 167 * 3600;
    if min_remaining_seconds > MAX_REMAINING_SECONDS {
        return Err(format!(
            "Weekly reset threshold must be between 0 and 167 hours (got {})",
            min_remaining_seconds / 3600
        ));
    }
    let mut file = load_accounts()?;
    file.settings.auto_reset_weekly_enabled = enabled;
    file.settings.auto_reset_weekly_min_remaining_seconds = min_remaining_seconds;
    save_accounts(&file)?;
    sync_settings_to_status_file(&file.settings);
    println!(
        "✅ Setting updated: auto_reset_weekly_enabled = {}, auto_reset_weekly_min_remaining_seconds = {}",
        enabled, min_remaining_seconds
    );
    Ok(())
}

/// Manually sets a multiplier override for an account.
pub fn set_account_multiplier(account_id: &str, multiplier: f64) -> Result<(), String> {
    if multiplier <= 0.0 {
        return Err("Multiplier must be greater than 0".to_string());
    }
    let mut file = load_accounts()?;
    let acc = file
        .accounts
        .iter_mut()
        .find(|a| {
            a.id == account_id
                || a.name
                    .as_deref()
                    .map(|n| n.eq_ignore_ascii_case(account_id))
                    .unwrap_or(false)
                || a.email.eq_ignore_ascii_case(account_id)
        })
        .ok_or_else(|| format!("Account '{}' not found", account_id))?;

    acc.plan_multiplier = Some(multiplier);
    acc.multiplier_is_manual = Some(true);
    let name = acc.display_name().to_string();
    save_accounts(&file)?;
    println!(
        "✅ Account '{}' multiplier manually set to {:.1}x",
        name, multiplier
    );
    Ok(())
}

/// Clears manual multiplier override and re-runs auto-detection.
pub fn reset_account_multiplier(account_id: &str) -> Result<(), String> {
    let mut file = load_accounts()?;
    let acc = file
        .accounts
        .iter_mut()
        .find(|a| {
            a.id == account_id
                || a.name
                    .as_deref()
                    .map(|n| n.eq_ignore_ascii_case(account_id))
                    .unwrap_or(false)
                || a.email.eq_ignore_ascii_case(account_id)
        })
        .ok_or_else(|| format!("Account '{}' not found", account_id))?;

    acc.multiplier_is_manual = None;
    acc.plan_multiplier = None;
    acc.last_multiplier_checked = None;
    let detected = crate::quota::detect_account_multiplier(acc);
    let name = acc.display_name().to_string();
    save_accounts(&file)?;
    println!(
        "✅ Account '{}' multiplier reset and auto-detected as {:.1}x",
        name, detected
    );
    Ok(())
}
