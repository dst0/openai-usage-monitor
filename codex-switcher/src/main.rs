mod daemon;
mod models;
mod oauth;
mod quota;
mod setup;
mod shim;
mod storage;
mod strategy;
mod switcher;

use clap::{Parser, Subcommand};
use quota::format_reset_duration;
use std::env;

#[derive(Parser)]
#[command(name = "codex-mon", version = "0.1.0", about = "OpenAI Codex Account Switcher & Quota Monitor")]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
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
    },
    /// Resume an active, interrupted, or credit-exhausted thread in ChatGPT
    Resume {
        /// Thread ID or URL (e.g. codex://threads/<id> or bare UUID). Defaults to most recent thread.
        thread_id: Option<String>,
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
}

fn print_status_table(refresh: bool) -> Result<(), String> {
    if refresh {
        println!("🔄 Refreshing quotas from OpenAI API...");
        let _ = daemon::refresh_quotas_and_status();
    } else if let Ok(mut accounts_file) = storage::load_accounts() {
        if let Ok(true) = daemon::sync_active_tokens(&mut accounts_file) {
            // Newly detected account or refreshed credentials: fetch quota and update status file
            let _ = daemon::refresh_quotas_and_status();
        }
    }

    let accounts_file = storage::load_accounts()?;
    if accounts_file.accounts.is_empty() {
        println!("No accounts configured. Run `codex-mon setup` or `codex-mon save-current main`.");
        return Ok(());
    }

    let active_id = accounts_file.active_account_id.as_deref().unwrap_or("");

    println!();
    println!("📊 OpenAI Codex Accounts & Rate Limits (Strategy: {})", accounts_file.settings.strategy);
    println!("---------------------------------------------------------------------------------------------------------");
    println!("{:<4} {:<12} {:<26} {:<11} {:<20} {:<12} {:<10} {:<7}", 
        "", "ACCOUNT", "EMAIL", "PLAN (MULT)", "5H SPRINT (EQ)", "RESET IN", "7D LIMIT", "CREDITS");
    println!("---------------------------------------------------------------------------------------------------------");

    for acc in &accounts_file.accounts {
        let is_active = acc.id == active_id;
        let prefix = if is_active { "→" } else { " " };
        
        let pct = acc.last_primary_percentage;
        let mult = acc.effective_multiplier();
        let tank_pct = if mult > 0.0 { (pct / mult).clamp(0.0, 100.0) } else { pct };
        let dot = if tank_pct > 50.0 {
            "🟢"
        } else if tank_pct > 15.0 {
            "🟡"
        } else {
            "🔴"
        };

        let bar = format_progress_bar(tank_pct);
        let reset_str = acc.last_reset_after_seconds
            .map(format_reset_duration)
            .unwrap_or_else(|| "--".to_string());

        let weekly_str = acc.last_weekly_percentage
            .map(|w| format!("{:.0}%", w))
            .unwrap_or_else(|| "--".to_string());

        let credits_str = acc.last_credits
            .map(|c| c.to_string())
            .unwrap_or_else(|| "0".to_string());

        let active_indicator = if is_active { format!("{} {}", prefix, dot) } else { format!("  {}", dot) };

        let plan_str = format!("{:<5} {:>2.0}x", acc.plan_type, mult);

        println!("{:<4} {:<12} {:<26} {:<11} {:<20} {:<12} {:<10} {:<7}",
            active_indicator,
            truncate_str(acc.display_name(), 12),
            truncate_str(&acc.email, 25),
            plan_str,
            format!("{} {:>5.0}%", bar, pct),
            reset_str,
            weekly_str,
            credits_str
        );

        if let Some(err) = &acc.last_error {
            println!("      ↳ ⚠️ Error: {}", err);
        }
    }

    println!("---------------------------------------------------------------------------------------------------------");
    println!("💡 Switch account: `codex-mon switch <name|id>` | Set multiplier: `codex-mon set-multiplier <name> <val>`");
    println!();
    Ok(())
}

fn format_progress_bar(pct: f64) -> String {
    let total = 8;
    let filled = ((pct / 100.0) * total as f64).round().clamp(0.0, total as f64) as usize;
    let empty = total - filled;
    format!("{}{}", "█".repeat(filled), "░".repeat(empty))
}

fn truncate_str(s: &str, max_len: usize) -> String {
    if s.len() <= max_len {
        s.to_string()
    } else {
        format!("{}…", &s[..max_len - 1])
    }
}

fn percent_encode_path(path: &str) -> String {
    let mut encoded = String::new();
    for byte in path.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

pub fn resolve_helps_path() -> std::path::PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| std::path::PathBuf::from("/"));
    let candidates = [
        home.join(".codex/helps.html"),
        home.join("Applications/Codex Monitor.app/Contents/Resources/helps.html"),
        std::path::PathBuf::from("/Applications/Codex Monitor.app/Contents/Resources/helps.html"),
        home.join("dev/openai-usage-monitor/resources/helps.html"),
    ];
    for p in &candidates {
        if p.exists() {
            return p.clone();
        }
    }
    home.join(".codex/helps.html")
}

pub fn open_helps_in_browser() -> Result<(), String> {
    let path = resolve_helps_path();
    let path_str = path.display().to_string();
    let encoded_path = percent_encode_path(&path_str);
    let target_url = format!("file://{}", encoded_path);
    println!("📖 Opening documentation: {}", target_url);

    #[cfg(target_os = "macos")]
    let status = std::process::Command::new("open").arg(&target_url).status();
    #[cfg(not(target_os = "macos"))]
    let status = std::process::Command::new("xdg-open").arg(&target_url).status();

    status
        .map_err(|e| format!("Failed to open browser: {}", e))
        .map(|_| ())
}

fn main() {
    let raw_args: Vec<String> = env::args().collect();
    // If invoked as "codex", directly execute shim wrapper
    if raw_args.first().map(|s| s.ends_with("/codex")).unwrap_or(false) {
        if let Err(e) = shim::run_codex_with_auto_switch(&raw_args[1..]) {
            eprintln!("[codex shim error] {}", e);
            std::process::exit(1);
        }
        return;
    }

    let cli = Cli::parse();

    let result = match cli.command {
        None => print_status_table(false),
        Some(Commands::Status { refresh }) => print_status_table(refresh),
        Some(Commands::Switch { account, no_restart, restart }) => {
            let accounts = storage::load_accounts().unwrap_or_default();
            let should_restart = (restart || accounts.settings.restart_app_on_switch) && !no_restart;
            let notify = accounts.settings.notify_on_switch;
            println!("🔄 Switching to account '{}'...", account);
            match switcher::switch_to_account(&account, should_restart, notify) {
                Ok(()) => {
                    println!("✅ Successfully switched to account '{}'!", account);
                    Ok(())
                }
                Err(e) => Err(e),
            }
        }
        Some(Commands::Resume { thread_id }) => {
            switcher::resume_thread_interactive(thread_id.as_deref())
        }
        Some(Commands::Rename { account, new_name, clear }) => {
            let target = if clear { None } else { Some(new_name.as_str()) };
            setup::rename_account(&account, target)
        }
        Some(Commands::Config { restart_app_on_switch, auto_switch_enabled }) => (|| {
            if let Some(val) = restart_app_on_switch {
                setup::set_config_restart_app_on_switch(val)?;
            }
            if let Some(val) = auto_switch_enabled {
                setup::set_config_auto_switch_enabled(val)?;
            }
            if restart_app_on_switch.is_none() && auto_switch_enabled.is_none() {
                let accounts = storage::load_accounts().unwrap_or_default();
                println!("restart_app_on_switch: {}", accounts.settings.restart_app_on_switch);
                println!("auto_switch_enabled: {}", accounts.settings.auto_switch_enabled);
            }
            Ok(())
        })(),
        Some(Commands::SetMultiplier { account, multiplier }) => {
            setup::set_account_multiplier(&account, multiplier)
        }
        Some(Commands::ResetMultiplier { account }) => {
            setup::reset_account_multiplier(&account)
        }
        Some(Commands::Setup) => setup::run_interactive_setup(),
        Some(Commands::Add { account_id }) => setup::login_and_add_account(&account_id),
        Some(Commands::SaveCurrent { account_id }) => setup::save_current_as(&account_id),
        Some(Commands::Remove { account_id }) => setup::remove_account(&account_id),
        Some(Commands::Daemon) => {
            daemon::run_daemon_loop();
            Ok(())
        }
        Some(Commands::Test) => {
            println!("Testing OpenAI Codex API Quotas...");
            match daemon::refresh_quotas_and_status() {
                Ok(()) => print_status_table(false),
                Err(e) => Err(e),
            }
        }
        Some(Commands::InstallShim) => shim::install_shim(),
        Some(Commands::Wrap { args }) => {
            shim::run_codex_with_auto_switch(&args)
        }
        Some(Commands::Helps) => open_helps_in_browser(),
    };

    if let Err(err) = result {
        eprintln!("❌ Error: {}", err);
        std::process::exit(1);
    }
}
