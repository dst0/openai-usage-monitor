use crate::quota::format_reset_duration;
use crate::{daemon, storage};

pub(super) struct StatusTableService;

impl StatusTableService {
    pub(super) fn print(refresh: bool) -> Result<(), String> {
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
            println!(
                "No accounts configured. Run `codex-mon setup` or `codex-mon save-current main`."
            );
            return Ok(());
        }

        let active_id = accounts_file.active_account_id.as_deref().unwrap_or("");

        println!();
        println!(
            "📊 OpenAI Codex Accounts & Rate Limits (Strategy: {})",
            accounts_file.settings.strategy
        );
        println!("---------------------------------------------------------------------------------------------------------");
        println!(
            "{:<4} {:<12} {:<26} {:<11} {:<20} {:<12} {:<10} {:<7}",
            "",
            "ACCOUNT",
            "EMAIL",
            "PLAN (MULT)",
            "5H SPRINT (EQ)",
            "RESET IN",
            "7D LIMIT",
            "CREDITS"
        );
        println!("---------------------------------------------------------------------------------------------------------");

        for acc in &accounts_file.accounts {
            let is_active = acc.id == active_id;
            let prefix = if is_active { "→" } else { " " };

            let pct = acc.last_primary_percentage;
            let mult = acc.effective_multiplier();
            let tank_pct = if mult > 0.0 {
                (pct / mult).clamp(0.0, 100.0)
            } else {
                pct
            };
            let dot = if tank_pct > 50.0 {
                "🟢"
            } else if tank_pct > 15.0 {
                "🟡"
            } else {
                "🔴"
            };

            let bar = format_progress_bar(tank_pct);
            let reset_str = acc
                .last_reset_after_seconds
                .map(format_reset_duration)
                .unwrap_or_else(|| "--".to_string());

            let weekly_str = acc
                .last_weekly_percentage
                .map(|w| format!("{:.0}%", w))
                .unwrap_or_else(|| "--".to_string());

            let credits_str = acc
                .last_credits
                .map(|c| c.to_string())
                .unwrap_or_else(|| "0".to_string());

            let active_indicator = if is_active {
                format!("{} {}", prefix, dot)
            } else {
                format!("  {}", dot)
            };

            let plan_str = format!("{:<5} {:>2.0}x", acc.plan_type, mult);

            println!(
                "{:<4} {:<12} {:<26} {:<11} {:<20} {:<12} {:<10} {:<7}",
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
                if acc.needs_relogin() {
                    let hint = acc.name.as_deref().unwrap_or(&acc.id);
                    println!(
                        "        🔑 Re-login required: run `cxi relogin \"{}\"`",
                        hint
                    );
                }
            }
        }

        println!("---------------------------------------------------------------------------------------------------------");
        println!("💡 Switch account: `codex-mon switch <name|id>` | Set multiplier: `codex-mon set-multiplier <name> <val>`");
        println!();
        Ok(())
    }
}

fn format_progress_bar(pct: f64) -> String {
    let total = 8;
    let filled = ((pct / 100.0) * total as f64)
        .round()
        .clamp(0.0, total as f64) as usize;
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
