use crate::account_command_service::AccountCommandService;
use crate::commands::Commands;
use crate::desktop_command_service::DesktopCommandService;
use crate::help_service::HelpService;
use crate::log_command_service::LogCommandService;
use crate::status_table_service::StatusTableService;
use crate::switch_command_service::SwitchCommandService;
use crate::{daemon, recovery, setup, shim, switcher};

pub(super) struct CommandDispatcher;

impl CommandDispatcher {
    pub(super) fn dispatch(command: Option<Commands>) -> Result<(), String> {
        match command {
            None => StatusTableService::print(false),
            Some(Commands::Status { refresh }) => StatusTableService::print(refresh),
            Some(Commands::Switch {
                account,
                no_restart,
                restart,
                trigger,
            }) => SwitchCommandService::switch_account(account, no_restart, restart, trigger),
            Some(Commands::Distribute {
                trigger,
                reason,
                dry_run,
                app_target,
                cli_target,
                no_restart,
                json,
            }) => SwitchCommandService::distribute(
                trigger, reason, dry_run, app_target, cli_target, no_restart, json,
            ),
            Some(Commands::Resume { thread_id }) => {
                switcher::resume_thread_interactive(thread_id.as_deref())
            }
            Some(Commands::Restart {
                delay_seconds,
                primary_thread,
            }) => switcher::restart_and_recover(delay_seconds, primary_thread),
            Some(Commands::Rename {
                account,
                new_name,
                clear,
            }) => AccountCommandService::rename(&account, &new_name, clear),
            Some(Commands::Config {
                restart_app_on_switch,
                auto_switch_enabled,
                auto_switch_business_only,
                auto_switch_business_priority,
                auto_reset_weekly_enabled,
                auto_reset_weekly_min_hours,
                preserve_window_bounds,
            }) => AccountCommandService::configure(
                restart_app_on_switch,
                auto_switch_enabled,
                auto_switch_business_only,
                auto_switch_business_priority,
                auto_reset_weekly_enabled,
                auto_reset_weekly_min_hours,
                preserve_window_bounds,
            ),
            Some(Commands::Window { action }) => DesktopCommandService::window(action),
            Some(Commands::SetMultiplier {
                account,
                multiplier,
            }) => setup::set_account_multiplier(&account, multiplier),
            Some(Commands::ResetMultiplier { account }) => {
                setup::reset_account_multiplier(&account)
            }
            Some(Commands::ResetAccount { account }) => setup::reset_account(&account),
            Some(Commands::Setup) => setup::run_interactive_setup(),
            Some(Commands::Add { account_id }) => setup::login_and_add_account(&account_id),
            Some(Commands::SaveCurrent { account_id }) => setup::save_current_as(&account_id),
            Some(Commands::Remove { account_id }) => setup::remove_account(&account_id),
            Some(Commands::Relogin {
                account,
                restart,
                no_restart,
            }) => AccountCommandService::relogin(account, restart, no_restart),
            Some(Commands::Daemon) => {
                daemon::run_daemon_loop();
                Ok(())
            }
            Some(Commands::Test) => {
                println!("Testing OpenAI Codex API Quotas...");
                daemon::refresh_quotas_and_status()?;
                StatusTableService::print(false)
            }
            Some(Commands::InstallShim) => shim::install_shim(),
            Some(Commands::Wrap { args }) => shim::run_codex_with_auto_switch(&args),
            Some(Commands::Helps) => HelpService::open_in_browser(),
            Some(Commands::Logs {
                lines,
                archives,
                rotate,
            }) => LogCommandService::logs(lines, archives, rotate),
            Some(Commands::RecoveryPreflight) => recovery::preflight_desktop_dispatch(),
            Some(Commands::MonitorLogs {
                install,
                remove,
                dry_run,
                purge_data,
                cancel,
            }) => LogCommandService::monitor_logs(install, remove, dry_run, purge_data, cancel),
        }
    }
}
