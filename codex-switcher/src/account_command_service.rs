use crate::{setup, storage};
use std::time::Duration;

/// How long `cxi config` waits in total for other holders of the registry
/// locks. Holds last milliseconds (network calls run outside the lock); a
/// holder that keeps it longer is stuck, and the command reports the registry
/// busy instead. Test builds use a short wait so the busy path runs quickly.
pub(super) const CONFIG_LOCK_WAIT: Duration = if cfg!(test) {
    Duration::from_millis(300)
} else {
    Duration::from_secs(10)
};

pub(super) struct AccountCommandService;

impl AccountCommandService {
    pub(super) fn rename(account: &str, new_name: &str, clear: bool) -> Result<(), String> {
        setup::rename_account(account, (!clear).then_some(new_name))
    }

    /// Saves the requested settings in one registry transaction, or prints
    /// them all when none is given. Every registry lock wait is bounded by
    /// `CONFIG_LOCK_WAIT`, so the command always finishes: the Menu Bar keeps
    /// its settings rows disabled until it does. Exit status 0 means the
    /// registry holds every requested value; a failure means it holds none.
    #[allow(clippy::too_many_arguments)]
    pub(super) fn configure(
        restart_app_on_switch: Option<bool>,
        auto_switch_enabled: Option<bool>,
        auto_switch_business_only: Option<bool>,
        auto_switch_business_priority: Option<bool>,
        auto_reset_weekly_enabled: Option<bool>,
        auto_reset_weekly_min_hours: Option<u64>,
        preserve_window_bounds: Option<bool>,
    ) -> Result<(), String> {
        let changes = setup::ConfigChanges {
            restart_app_on_switch,
            auto_switch_enabled,
            auto_switch_business_only,
            auto_switch_business_priority,
            preserve_window_bounds,
            auto_reset_weekly_enabled,
            auto_reset_weekly_min_hours,
        };
        storage::with_lock_wait_budget(CONFIG_LOCK_WAIT, || {
            if changes.is_empty() {
                Self::print_configuration()
            } else {
                setup::set_config(changes)
            }
        })
    }

    pub(super) fn relogin(account: String, restart: bool, no_restart: bool) -> Result<(), String> {
        let target = if account.trim().is_empty() {
            let accounts = storage::load_accounts().unwrap_or_default();
            if let Some(active_id) = accounts.active_account_id {
                active_id
            } else if accounts.accounts.len() == 1 {
                accounts.accounts[0].id.clone()
            } else {
                return Err(
                    "Please specify an account to re-login (e.g. `codex-mon relogin <name|email>`)"
                        .into(),
                );
            }
        } else {
            account
        };
        setup::relogin_account(&target, restart, no_restart)
    }

    /// Prints the saved settings. An unreadable or busy registry is an error,
    /// never a list of defaults; with no registry at all, the defaults are
    /// what the Rust core applies, so those print.
    fn print_configuration() -> Result<(), String> {
        let accounts = storage::load_accounts()?;
        println!(
            "restart_app_on_switch: {}",
            accounts.settings.restart_app_on_switch
        );
        println!(
            "auto_switch_enabled: {}",
            accounts.settings.auto_switch_enabled
        );
        println!(
            "auto_switch_business_only: {}",
            accounts.settings.auto_switch_business_only
        );
        println!(
            "auto_switch_business_priority: {}",
            accounts.settings.auto_switch_business_priority
        );
        println!(
            "auto_reset_weekly_enabled: {}",
            accounts.settings.auto_reset_weekly_enabled
        );
        println!(
            "auto_reset_weekly_min_hours: {}",
            accounts.settings.auto_reset_weekly_min_remaining_seconds / 3600
        );
        println!(
            "preserve_window_bounds_on_restart: {}",
            accounts.settings.preserve_window_bounds_on_restart
        );
        Ok(())
    }
}

#[cfg(test)]
#[path = "account_command_service.test.rs"]
mod tests;
