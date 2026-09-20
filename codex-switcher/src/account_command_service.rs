use crate::{setup, storage};

pub(super) struct AccountCommandService;

impl AccountCommandService {
    pub(super) fn rename(account: &str, new_name: &str, clear: bool) -> Result<(), String> {
        setup::rename_account(account, (!clear).then_some(new_name))
    }

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
        if let Some(value) = restart_app_on_switch {
            setup::set_config_restart_app_on_switch(value)?;
        }
        if let Some(value) = auto_switch_enabled {
            setup::set_config_auto_switch_enabled(value)?;
        }
        if let Some(value) = auto_switch_business_only {
            setup::set_config_auto_switch_business_only(value)?;
        }
        if let Some(value) = auto_switch_business_priority {
            setup::set_config_auto_switch_business_priority(value)?;
        }
        if let Some(value) = preserve_window_bounds {
            setup::set_config_preserve_window_bounds(value)?;
        }
        if auto_reset_weekly_enabled.is_some() || auto_reset_weekly_min_hours.is_some() {
            let current = storage::load_accounts().unwrap_or_default();
            let enabled =
                auto_reset_weekly_enabled.unwrap_or(current.settings.auto_reset_weekly_enabled);
            let threshold_hours = auto_reset_weekly_min_hours
                .unwrap_or(current.settings.auto_reset_weekly_min_remaining_seconds / 3600);
            setup::set_config_auto_reset_weekly(enabled, threshold_hours * 3600)?;
        }
        if restart_app_on_switch.is_none()
            && auto_switch_enabled.is_none()
            && auto_switch_business_only.is_none()
            && auto_switch_business_priority.is_none()
            && auto_reset_weekly_enabled.is_none()
            && auto_reset_weekly_min_hours.is_none()
            && preserve_window_bounds.is_none()
        {
            Self::print_configuration();
        }
        Ok(())
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

    fn print_configuration() {
        let accounts = storage::load_accounts().unwrap_or_default();
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
    }
}
