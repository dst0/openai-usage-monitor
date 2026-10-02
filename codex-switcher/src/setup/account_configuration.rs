use crate::models::Settings;
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

/// The settings one `cxi config` call changes; `None` keeps the saved value.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct ConfigChanges {
    pub restart_app_on_switch: Option<bool>,
    pub auto_switch_enabled: Option<bool>,
    pub auto_switch_business_only: Option<bool>,
    pub auto_switch_business_priority: Option<bool>,
    pub preserve_window_bounds: Option<bool>,
    pub auto_reset_weekly_enabled: Option<bool>,
    /// A threshold of zero lets the weekly policy act whenever the weekly pool
    /// is exactly exhausted; otherwise that many hours must remain before the
    /// normal weekly reset.
    pub auto_reset_weekly_min_hours: Option<u64>,
}

const MAX_WEEKLY_RESET_HOURS: u64 = 167;

impl ConfigChanges {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    fn changes_weekly_reset(&self) -> bool {
        self.auto_reset_weekly_enabled.is_some() || self.auto_reset_weekly_min_hours.is_some()
    }

    /// Whether the status cache carries any of these settings.
    fn changes_status_cache(&self) -> bool {
        self.auto_switch_enabled.is_some()
            || self.auto_switch_business_only.is_some()
            || self.auto_switch_business_priority.is_some()
            || self.changes_weekly_reset()
    }

    /// Applies the given values in flag order. Turning business-only or
    /// business priority on also turns automatic switching on and the other
    /// business mode off.
    fn apply(&self, settings: &mut Settings) {
        if let Some(value) = self.restart_app_on_switch {
            settings.restart_app_on_switch = value;
        }
        if let Some(value) = self.auto_switch_enabled {
            settings.auto_switch_enabled = value;
        }
        if let Some(value) = self.auto_switch_business_only {
            settings.auto_switch_business_only = value;
            if value {
                settings.auto_switch_business_priority = false;
                settings.auto_switch_enabled = true;
            }
        }
        if let Some(value) = self.auto_switch_business_priority {
            settings.auto_switch_business_priority = value;
            if value {
                settings.auto_switch_business_only = false;
                settings.auto_switch_enabled = true;
            }
        }
        if let Some(value) = self.preserve_window_bounds {
            settings.preserve_window_bounds_on_restart = value;
        }
        if let Some(value) = self.auto_reset_weekly_enabled {
            settings.auto_reset_weekly_enabled = value;
        }
        if let Some(hours) = self.auto_reset_weekly_min_hours {
            settings.auto_reset_weekly_min_remaining_seconds = hours * 3600;
        }
    }
}

/// Saves every given setting in one locked registry transaction, merged into
/// what the registry holds at that moment: a busy or failing registry saves
/// none of them, and the daemon never sees half of a change. Success means
/// the registry holds them all.
pub fn set_config(changes: ConfigChanges) -> Result<(), String> {
    set_config_with_hook(changes, || Ok(()))
}

fn set_config_with_hook(
    changes: ConfigChanges,
    before_save: impl FnOnce() -> Result<(), String>,
) -> Result<(), String> {
    if let Some(hours) = changes
        .auto_reset_weekly_min_hours
        .filter(|hours| *hours > MAX_WEEKLY_RESET_HOURS)
    {
        return Err(format!(
            "Weekly reset threshold must be between 0 and 167 hours (got {})",
            hours
        ));
    }
    before_save()?;
    let saved = update_accounts_atomically(|file| {
        changes.apply(&mut file.settings);
        Ok(())
    })?
    .settings;
    if changes.changes_status_cache() {
        sync_status_cache_or_defer()?;
    }
    print_saved(&changes, &saved);
    Ok(())
}

fn print_saved(changes: &ConfigChanges, saved: &Settings) {
    let flags = [
        (
            "restart_app_on_switch",
            changes.restart_app_on_switch,
            saved.restart_app_on_switch,
        ),
        (
            "auto_switch_enabled",
            changes.auto_switch_enabled,
            saved.auto_switch_enabled,
        ),
        (
            "auto_switch_business_only",
            changes.auto_switch_business_only,
            saved.auto_switch_business_only,
        ),
        (
            "auto_switch_business_priority",
            changes.auto_switch_business_priority,
            saved.auto_switch_business_priority,
        ),
        (
            "preserve_window_bounds_on_restart",
            changes.preserve_window_bounds,
            saved.preserve_window_bounds_on_restart,
        ),
    ];
    for (name, _, value) in flags.iter().filter(|(_, given, _)| given.is_some()) {
        println!("✅ Setting updated: {} = {}", name, value);
    }
    if changes.changes_weekly_reset() {
        println!(
            "✅ Setting updated: auto_reset_weekly_enabled = {}, auto_reset_weekly_min_remaining_seconds = {}",
            saved.auto_reset_weekly_enabled, saved.auto_reset_weekly_min_remaining_seconds
        );
    }
}

#[cfg(test)]
#[path = "account_configuration.test.rs"]
mod tests;

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
