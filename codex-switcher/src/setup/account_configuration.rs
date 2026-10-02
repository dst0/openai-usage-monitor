use crate::storage::{
    load_accounts, sync_settings_to_status_file, update_accounts_atomically, REGISTRY_BUSY,
};

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
    let clean_new = new_name.map(str::trim).filter(|s| !s.is_empty());
    let (mut updated_id, mut updated_name) = (String::new(), None);
    update_accounts_atomically(|file| {
        let idx = crate::switcher::resolve_target_account_idx(&file.accounts, query)?;
        if let Some(name) = clean_new {
            for (i, account) in file.accounts.iter().enumerate() {
                if i != idx {
                    if let Some(existing_name) = &account.name {
                        if existing_name.trim().eq_ignore_ascii_case(name) {
                            return Err(format!(
                                "Nickname '{}' is already used by another account ({})",
                                name, account.email
                            ));
                        }
                    }
                }
            }
        }
        let account = &mut file.accounts[idx];
        account.name = clean_new.map(str::to_string);
        updated_id = account.id.clone();
        updated_name = account.name.clone();
        Ok(())
    })?;

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
    update_accounts_atomically(|file| {
        file.settings.restart_app_on_switch = enabled;
        Ok(())
    })?;
    println!("✅ Setting updated: restart_app_on_switch = {}", enabled);
    Ok(())
}

/// Updates the preserve_window_bounds_on_restart setting in accounts.json.
pub fn set_config_preserve_window_bounds(enabled: bool) -> Result<(), String> {
    set_config_preserve_window_bounds_with_hook(enabled, || Ok(()))
}

fn set_config_preserve_window_bounds_with_hook(
    enabled: bool,
    before_save: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    before_save()?;
    update_accounts_atomically(|file| {
        file.settings.preserve_window_bounds_on_restart = enabled;
        Ok(())
    })?;
    println!(
        "✅ Setting updated: preserve_window_bounds_on_restart = {}",
        enabled
    );
    Ok(())
}

#[cfg(test)]
#[path = "account_configuration.test.rs"]
mod tests;

/// Updates the auto_switch_enabled setting in accounts.json.
pub fn set_config_auto_switch_enabled(enabled: bool) -> Result<(), String> {
    update_accounts_atomically(|file| {
        file.settings.auto_switch_enabled = enabled;
        Ok(())
    })?;
    sync_status_cache_or_defer()?;
    println!("✅ Setting updated: auto_switch_enabled = {}", enabled);
    Ok(())
}

/// Updates the auto_switch_business_only setting in accounts.json.
pub fn set_config_auto_switch_business_only(enabled: bool) -> Result<(), String> {
    update_accounts_atomically(|file| {
        file.settings.auto_switch_business_only = enabled;
        if enabled {
            file.settings.auto_switch_business_priority = false;
            file.settings.auto_switch_enabled = true;
        }
        Ok(())
    })?;
    sync_status_cache_or_defer()?;
    println!(
        "✅ Setting updated: auto_switch_business_only = {}",
        enabled
    );
    Ok(())
}

/// Updates the auto_switch_business_priority setting in accounts.json.
pub fn set_config_auto_switch_business_priority(enabled: bool) -> Result<(), String> {
    update_accounts_atomically(|file| {
        file.settings.auto_switch_business_priority = enabled;
        if enabled {
            file.settings.auto_switch_business_only = false;
            file.settings.auto_switch_enabled = true;
        }
        Ok(())
    })?;
    sync_status_cache_or_defer()?;
    println!(
        "✅ Setting updated: auto_switch_business_priority = {}",
        enabled
    );
    Ok(())
}

/// Updates the opt-in weekly reset-credit policy. A threshold of zero means
/// that the policy may act whenever the weekly pool is exactly exhausted;
/// non-zero thresholds require that many hours to remain before the normal
/// weekly reset. A value left out keeps the saved one, merged inside the same
/// locked transaction, so a busy or concurrently changed registry can never
/// turn into a default here. Keeping this as one atomic accounts.json write
/// prevents the daemon from observing a half-updated policy when the menu
/// changes both values together.
pub fn set_config_auto_reset_weekly(
    enabled: Option<bool>,
    min_remaining_hours: Option<u64>,
) -> Result<(), String> {
    const MAX_REMAINING_HOURS: u64 = 167;
    if let Some(hours) = min_remaining_hours.filter(|hours| *hours > MAX_REMAINING_HOURS) {
        return Err(format!(
            "Weekly reset threshold must be between 0 and 167 hours (got {})",
            hours
        ));
    }
    let saved = update_accounts_atomically(|file| {
        if let Some(enabled) = enabled {
            file.settings.auto_reset_weekly_enabled = enabled;
        }
        if let Some(hours) = min_remaining_hours {
            file.settings.auto_reset_weekly_min_remaining_seconds = hours * 3600;
        }
        Ok(())
    })?;
    sync_status_cache_or_defer()?;
    println!(
        "✅ Setting updated: auto_reset_weekly_enabled = {}, auto_reset_weekly_min_remaining_seconds = {}",
        saved.settings.auto_reset_weekly_enabled,
        saved.settings.auto_reset_weekly_min_remaining_seconds
    );
    Ok(())
}

/// Copies the saved settings into the status cache. The registry is already
/// saved, so a lock that stays busy past this command's wait budget only
/// delays the copy: every status write applies the registry's settings, so
/// the daemon's next one catches the cache up. Any other failure is reported.
fn sync_status_cache_or_defer() -> Result<(), String> {
    match sync_settings_to_status_file() {
        Err(error) if error == REGISTRY_BUSY => {
            eprintln!(
                "⚠️  Setting saved; the status cache will show it after the next status update ({})",
                REGISTRY_BUSY
            );
            Ok(())
        }
        result => result,
    }
}

/// Manually sets a multiplier override for an account.
pub fn set_account_multiplier(account_id: &str, multiplier: f64) -> Result<(), String> {
    if multiplier <= 0.0 {
        return Err("Multiplier must be greater than 0".to_string());
    }
    let mut name = String::new();
    update_accounts_atomically(|file| {
        let index = crate::switcher::resolve_target_account_idx(&file.accounts, account_id)?;
        let account = &mut file.accounts[index];
        account.plan_multiplier = Some(multiplier);
        account.multiplier_is_manual = Some(true);
        name = account.display_name().to_string();
        Ok(())
    })?;
    println!(
        "✅ Account '{}' multiplier manually set to {:.1}x",
        name, multiplier
    );
    Ok(())
}

/// Clears manual multiplier override and re-runs auto-detection.
pub fn reset_account_multiplier(account_id: &str) -> Result<(), String> {
    let snapshot = load_accounts()?;
    let index = crate::switcher::resolve_target_account_idx(&snapshot.accounts, account_id)?;
    let original = &snapshot.accounts[index];
    let mut updated = original.clone();
    updated.multiplier_is_manual = None;
    updated.plan_multiplier = None;
    updated.last_multiplier_checked = None;
    let detected = crate::quota::detect_account_multiplier(&mut updated);
    let name = updated.display_name().to_string();
    update_accounts_atomically(|file| {
        let current = file
            .accounts
            .iter_mut()
            .find(|account| account.id == original.id)
            .ok_or("Account changed during multiplier reset")?;
        if current.tokens != original.tokens
            || current.plan_type != original.plan_type
            || current.account_id != original.account_id
            || current.multiplier_is_manual != original.multiplier_is_manual
            || current.plan_multiplier != original.plan_multiplier
        {
            return Err("Account changed during multiplier reset; please retry".into());
        }
        current.multiplier_is_manual = updated.multiplier_is_manual;
        current.plan_multiplier = updated.plan_multiplier;
        current.last_multiplier_checked = updated.last_multiplier_checked.clone();
        current.organization_name = updated.organization_name.clone();
        Ok(())
    })?;
    println!(
        "✅ Account '{}' multiplier reset and auto-detected as {:.1}x",
        name, detected
    );
    Ok(())
}
