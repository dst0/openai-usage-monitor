use crate::storage::load_accounts;
use crate::strategy::{needs_switch, select_best_switch};
use crate::switcher::{dispatch_self_restart, is_codex_app_running, switch_to_account, SwitchTrigger};
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

pub const REAL_CODEX_PATH: &str = "/Applications/ChatGPT.app/Contents/Resources/codex";

fn command_uses_account(args: &[String]) -> bool {
    !matches!(
        args.first().map(String::as_str),
        Some("--version" | "-V" | "--help" | "-h" | "help" | "completion")
    )
}

fn auto_switch_allowed(enabled: bool, depleted: bool, cooldown_active: bool) -> bool {
    enabled && (!cooldown_active || depleted)
}

pub fn run_codex_with_auto_switch(args: &[String]) -> Result<(), String> {
    // 1. Quick check if current account needs switch
    if command_uses_account(args) {
        if let Ok(accounts_file) = load_accounts() {
            let active_id = accounts_file.active_account_id.as_deref();
            if let Some(active) = accounts_file
                .accounts
                .iter()
                .find(|a| Some(a.id.as_str()) == active_id)
            {
                let threshold = accounts_file.settings.switch_threshold_percent;
                let biz_priority = accounts_file.settings.auto_switch_business_priority;
                let depleted = active.last_primary_percentage <= threshold;
                let cooldown_active = crate::recovery::automation_cooldown_remaining()?.is_some();
                if auto_switch_allowed(
                    accounts_file.settings.auto_switch_enabled,
                    depleted,
                    cooldown_active,
                ) && needs_switch(active, threshold, biz_priority, &accounts_file.accounts)
                {
                    if biz_priority
                        && !active.is_business()
                        && active.last_primary_percentage > threshold
                    {
                        eprintln!(
                        "[codex-mon] ⚡ Active account '{}' is non-business ({:.0}%). Preempting to business account...",
                        active.id, active.last_primary_percentage
                    );
                    } else {
                        eprintln!(
                        "[codex-mon] ⚠️ Active account '{}' quota is {:.0}%. Checking for available account...",
                        active.id, active.last_primary_percentage
                    );
                    }
                    if let Some(next_id) = select_best_switch(
                        active_id,
                        &accounts_file.accounts,
                        threshold,
                        &accounts_file.settings.strategy,
                        accounts_file.settings.auto_switch_business_only,
                        accounts_file.settings.auto_switch_business_priority,
                    ) {
                        eprintln!("[codex-mon] 🔄 Switching to '{}'...", next_id);
                        let should_restart =
                            accounts_file.settings.restart_app_on_switch && is_codex_app_running();
                        if should_restart
                            && dispatch_self_restart(&[
                                "switch".into(),
                                next_id.clone(),
                                "--restart".into(),
                                "--trigger".into(),
                                "shim".into(),
                            ])?
                        {
                            return Err(
                            "Detached account switch scheduled; Codex will restart and recover this task"
                                .into(),
                        );
                        }
                        switch_to_account(
                            &next_id,
                            should_restart,
                            accounts_file.settings.notify_on_switch,
                            SwitchTrigger::Shim,
                        )?;
                    }
                }
            }
        }
    }

    // 2. Execute real codex binary via exec (replaces process)
    let mut cmd = Command::new(REAL_CODEX_PATH);
    cmd.args(args);
    let err = cmd.exec();
    Err(format!("Failed to exec {}: {}", REAL_CODEX_PATH, err))
}

pub fn install_shim() -> Result<(), String> {
    let local_bin = dirs::home_dir()
        .map(|h| h.join(".local").join("bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));

    std::fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?;

    let symlink_path = local_bin.join("codex");
    if symlink_path.exists() || symlink_path.is_symlink() {
        let _ = std::fs::remove_file(&symlink_path);
    }

    // Point to our codex-mon binary with alias or wrapper
    let mon_path = local_bin.join("codex-mon");
    if mon_path.exists() {
        std::os::unix::fs::symlink(&mon_path, &symlink_path).map_err(|e| {
            format!(
                "Failed to symlink {:?} to {:?}: {}",
                mon_path, symlink_path, e
            )
        })?;
        println!("✅ Installed codex CLI shim at {:?}", symlink_path);
    } else {
        std::os::unix::fs::symlink(REAL_CODEX_PATH, &symlink_path).map_err(|e| {
            format!(
                "Failed to symlink {:?} to {:?}: {}",
                REAL_CODEX_PATH, symlink_path, e
            )
        })?;
        println!("✅ Symlinked real codex to {:?}", symlink_path);
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_commands_never_trigger_account_automation() {
        assert!(!command_uses_account(&["--version".into()]));
        assert!(!command_uses_account(&["help".into()]));
        assert!(command_uses_account(&["exec".into(), "task".into()]));
    }

    #[test]
    fn disabled_auto_switch_is_always_honored() {
        assert!(!auto_switch_allowed(false, false, false));
        assert!(!auto_switch_allowed(false, true, false));
    }

    #[test]
    fn cooldown_defers_preemption_but_not_real_depletion() {
        assert!(!auto_switch_allowed(true, false, true));
        assert!(auto_switch_allowed(true, true, true));
    }
}
