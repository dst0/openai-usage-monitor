use crate::codex_binary_path::{resolve_real_codex_bin, resolve_real_codex_bin_avoiding};
use crate::distribution::{
    AutomaticDistributionService, AutomaticDistributionSource, DistributionCoordinator,
    DistributionExecutor, DistributionOutcome, DistributionStatus,
};
use crate::models::AccountsFile;
use crate::storage::load_accounts;
use std::os::unix::process::CommandExt;
use std::path::PathBuf;
use std::process::Command;

fn command_uses_account(args: &[String]) -> bool {
    !matches!(
        args.first().map(String::as_str),
        Some("--version" | "-V" | "--help" | "-h" | "help" | "completion")
    )
}

pub fn run_codex_with_auto_switch(args: &[String]) -> Result<(), String> {
    // Resolve before automatic distribution: a missing bundled CLI must not
    // switch accounts and then fail while trying to launch the real command.
    let real_codex = resolve_real_codex_bin()?;
    if command_uses_account(args) {
        if let Ok(accounts_file) = load_accounts() {
            let coordinator = DistributionCoordinator::new();
            if let Some(outcome) =
                coordinate_wrapper_automatic_distribution_with(&coordinator, args, &accounts_file)?
            {
                match outcome.status {
                    DistributionStatus::Success => {
                        eprintln!("[codex-mon] Automatic account distribution completed")
                    }
                    DistributionStatus::PartialSuccess => eprintln!(
                        "[codex-mon] Automatic account distribution completed with incomplete recovery"
                    ),
                    DistributionStatus::DeferredCooldown
                    | DistributionStatus::DeferredInFlight => {
                        eprintln!("[codex-mon] Automatic account distribution deferred")
                    }
                    DistributionStatus::NoActionNeeded => {}
                    DistributionStatus::Failed => {
                        return Err("Automatic account distribution failed".to_string())
                    }
                }
            }
        }
    }

    let mut cmd = Command::new(&real_codex);
    cmd.args(args);
    let err = cmd.exec();
    Err(format!("Failed to exec {}: {}", real_codex.display(), err))
}

pub(crate) fn coordinate_wrapper_automatic_distribution_with<E: DistributionExecutor>(
    executor: &E,
    args: &[String],
    accounts_file: &AccountsFile,
) -> Result<Option<DistributionOutcome>, String> {
    if !command_uses_account(args) {
        return Ok(None);
    }
    AutomaticDistributionService::new(executor).execute(
        AutomaticDistributionSource::WrapperPreflight,
        accounts_file,
        false,
    )
}

pub fn install_shim() -> Result<(), String> {
    let local_bin = dirs::home_dir()
        .map(|h| h.join(".local").join("bin"))
        .unwrap_or_else(|| PathBuf::from("/usr/local/bin"));

    std::fs::create_dir_all(&local_bin).map_err(|e| e.to_string())?;

    let symlink_path = local_bin.join("codex");
    if symlink_path
        .symlink_metadata()
        .is_ok_and(|metadata| !metadata.file_type().is_symlink())
    {
        return Err(format!(
            "Cannot install codex shim over existing non-symlink {}",
            symlink_path.display()
        ));
    }
    let real_codex = resolve_real_codex_bin_avoiding(Some(&symlink_path)).map_err(|error| {
        format!("Cannot install codex shim: {error}; preserve existing codex command")
    })?;
    if symlink_path.exists() || symlink_path.is_symlink() {
        std::fs::remove_file(&symlink_path).map_err(|error| error.to_string())?;
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
        std::os::unix::fs::symlink(&real_codex, &symlink_path).map_err(|e| {
            format!(
                "Failed to symlink {:?} to {:?}: {}",
                real_codex, symlink_path, e
            )
        })?;
        println!("✅ Symlinked real codex to {:?}", symlink_path);
    }

    Ok(())
}

#[cfg(test)]
#[path = "distribution/shim_automatic_preflight.test.rs"]
mod tests;
