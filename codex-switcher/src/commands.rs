use super::window_action::WindowAction;
use clap::Subcommand;

#[derive(Subcommand)]
pub(super) enum Commands {
    /// Show current status and quota for all accounts
    Status {
        /// Force refresh from OpenAI API
        #[arg(short, long)]
        refresh: bool,
    },
    /// Switch active account for Codex CLI and Desktop App
    Switch {
        /// Account ID to switch to
        account: String,
        /// Do not restart ChatGPT desktop app
        #[arg(long)]
        no_restart: bool,
        /// Force restart ChatGPT desktop app on switch
        #[arg(long)]
        restart: bool,
        /// Switch trigger reason (user, auto, shim)
        #[arg(long, hide = true, default_value = "user")]
        trigger: String,
    },
    /// Atomically distribute accounts between ChatGPT Desktop App and Codex CLI
    #[command(alias = "auto-distribute")]
    Distribute {
        /// Trigger type (auto or user)
        #[arg(long, default_value = "user")]
        trigger: String,
        /// Reason for triggering distribution
        #[arg(long, default_value = "manual_invocation")]
        reason: String,
        /// Dry-run mode: evaluate decision without applying changes
        #[arg(long)]
        dry_run: bool,
        /// Preferred account ID for Desktop App
        #[arg(long)]
        app_target: Option<String>,
        /// Preferred account ID for CLI
        #[arg(long)]
        cli_target: Option<String>,
        /// Do not restart Desktop App even if App account changes
        #[arg(long)]
        no_restart: bool,
        /// Output result as JSON
        #[arg(long)]
        json: bool,
    },
    /// Resume an active, interrupted, or credit-exhausted thread in ChatGPT
    Resume {
        /// Thread ID or URL (e.g. codex://threads/<id> or bare UUID). Defaults to most recent thread.
        thread_id: Option<String>,
    },
    /// Restart Codex and verify recovery, without changing accounts (supports self-restart)
    Restart {
        /// Delay before restart (useful for an independent launchd worker)
        #[arg(long, default_value_t = 0, value_parser = clap::value_parser!(u64).range(0..=60))]
        delay_seconds: u64,
        /// Restore this task first when running a self-restart
        #[arg(long)]
        primary_thread: Option<String>,
    },
    /// Rename an account label/nickname
    Rename {
        /// Current account ID, nickname, or email
        account: String,
        /// New nickname/label for this account
        #[arg(default_value = "")]
        new_name: String,
        /// Clear nickname (reverts to email-based display)
        #[arg(long)]
        clear: bool,
    },
    /// Configure settings (e.g. restart-app-on-switch, auto-switch-enabled)
    Config {
        /// Enable or disable restarting ChatGPT desktop app on switch
        #[arg(long)]
        restart_app_on_switch: Option<bool>,
        /// Enable or disable automatic account switching on quota exhaustion
        #[arg(long)]
        auto_switch_enabled: Option<bool>,
        /// Enable or disable auto-switch only across business accounts
        #[arg(long)]
        auto_switch_business_only: Option<bool>,
        /// Enable or disable auto-switch prioritizing business accounts first
        #[arg(long)]
        auto_switch_business_priority: Option<bool>,
        /// Enable or disable spending one reset credit for a blocked weekly quota
        #[arg(long)]
        auto_reset_weekly_enabled: Option<bool>,
        /// Hours that must remain before the ordinary weekly reset (0 = always)
        #[arg(long, value_parser = clap::value_parser!(u64).range(0..=167))]
        auto_reset_weekly_min_hours: Option<u64>,
        /// Remember and restore Codex Desktop window location and size across restart/switch
        #[arg(long)]
        preserve_window_bounds: Option<bool>,
    },
    /// Set custom multiplier override for an account (e.g. 20 for Pro 20x, 5 for Pro 5x / Business Premium)
    SetMultiplier {
        /// Current account ID, nickname, or email
        account: String,
        /// Multiplier value (e.g. 20.0 or 5.0)
        multiplier: f64,
    },
    /// Reset multiplier to automatic detection based on account plan and entitlement
    ResetMultiplier {
        /// Current account ID, nickname, or email
        account: String,
    },
    /// Consume an available rate-limit reset credit to restore account quota
    ResetAccount {
        /// Current account ID, nickname, or email
        account: String,
    },
    /// Interactive account setup wizard
    Setup,
    /// Log in via browser and add as a named account
    Add {
        /// ID to assign (e.g. personal, work, secondary)
        #[arg(default_value = "")]
        account_id: String,
    },
    /// Save current active session from ~/.codex/auth.json as a named account
    SaveCurrent {
        /// ID to assign (e.g. main, work, personal)
        #[arg(default_value = "")]
        account_id: String,
    },
    /// Remove an account from configuration
    Remove {
        /// Account ID to remove
        account_id: String,
    },
    /// Re-authenticate an existing account via browser login
    Relogin {
        /// Account ID, nickname, or email to re-authenticate (defaults to active account)
        #[arg(default_value = "")]
        account: String,
        /// Force restart ChatGPT desktop app on re-login
        #[arg(long)]
        restart: bool,
        /// Do not restart ChatGPT desktop app
        #[arg(long)]
        no_restart: bool,
    },
    /// Run background monitor and auto-switcher daemon
    Daemon,
    /// Run API connectivity test
    Test,
    /// Install codex CLI shim to ~/.local/bin/codex
    InstallShim,
    /// Wrap and run codex command with automatic quota check & switch
    Wrap {
        #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
        args: Vec<String>,
    },
    /// Open interactive HTML guide in default browser
    Helps,
    /// View switcher action logs or manage Brotli-compressed archives
    Logs {
        /// Number of recent lines to display (default: 40)
        #[arg(short = 'n', long, default_value_t = 40)]
        lines: usize,
        /// List all Brotli-compressed log archives
        #[arg(long)]
        archives: bool,
        /// Force immediate log rotation and Brotli compression
        #[arg(long)]
        rotate: bool,
    },
    /// Inspect or manage Codex Desktop window position and size persistence
    Window {
        #[command(subcommand)]
        action: Option<WindowAction>,
    },
    /// Diagnose the Desktop-owned recovery transport without changing a task
    #[command(hide = true)]
    RecoveryPreflight,
    /// Internal fd-anchored Monitor log setup and cleanup
    #[command(name = "monitor-logs", hide = true)]
    MonitorLogs {
        /// Prepare private active streams before launchd can create them
        #[arg(long)]
        install: bool,
        /// Remove Monitor-owned logs and optional account data
        #[arg(long)]
        remove: bool,
        /// Print the exact Monitor-owned log cleanup inventory
        #[arg(long)]
        dry_run: bool,
        /// Include the regular Monitor account registry in cleanup
        #[arg(long)]
        purge_data: bool,
        /// Arm the restart cancellation marker safely
        #[arg(long)]
        cancel: bool,
    },
}
