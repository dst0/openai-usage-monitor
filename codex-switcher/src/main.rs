mod account_command_service;
mod auto_reset;
mod cli;
mod command_dispatcher;
mod commands;
mod daemon;
mod desktop_command_service;
pub mod distribution;
mod help_service;
mod log_command_service;
pub mod logger;
mod logger_archive_info;
mod logger_archive_result;
mod models;
mod oauth;
mod quota;
mod recovery;
pub mod recovery_banner;
mod setup;
mod shim;
mod status_table_service;
mod storage;
mod strategy;
mod switch_command_service;
mod switcher;
mod window_action;

use clap::Parser;
use cli::Cli;
use command_dispatcher::CommandDispatcher;
use std::env;

fn main() {
    let raw_args: Vec<String> = env::args().collect();
    let has_restart_worker_marker = std::env::var_os("CODEX_RESTART_WORKER").is_some();
    let has_restart_operation = std::env::var_os("CODEX_RESTART_OPERATION").is_some();
    let is_restart_worker = raw_args.get(1).map(String::as_str) == Some("--restart-worker")
        && has_restart_worker_marker
        && has_restart_operation;
    if !is_restart_worker {
        // Older launches leaked both variables into ordinary desktop shells.
        // The pair alone must not skip commands or authorize inline restarts.
        std::env::remove_var("CODEX_RESTART_WORKER");
        std::env::remove_var("CODEX_RESTART_OPERATION");
    }

    // If invoked as "codex", directly execute shim wrapper
    if raw_args
        .first()
        .map(|s| s.ends_with("/codex"))
        .unwrap_or(false)
    {
        if let Err(e) = shim::run_codex_with_auto_switch(&raw_args[1..]) {
            eprintln!("[codex shim error] {}", e);
            std::process::exit(1);
        }
        return;
    }

    let cli = Cli::parse();
    if cli.restart_worker && !is_restart_worker {
        eprintln!("WORKER_REJECTED reason=missing one-shot invocation context");
        std::process::exit(1);
    }

    if is_restart_worker {
        match recovery::claim_restart_operation() {
            Ok(true) => {}
            Ok(false) => {
                crate::runtime_print!("WORKER_DUPLICATE_SKIPPED");
                return;
            }
            Err(error) => {
                crate::runtime_error!("WORKER_REJECTED reason={error}");
                return;
            }
        }
    }

    let result = CommandDispatcher::dispatch(cli.command);

    if let Err(err) = result {
        if is_restart_worker {
            crate::runtime_error!("❌ Error: {}", err);
            // `launchctl submit` retries a job that exits nonzero. The detailed
            // failure remains in the 0600 run log; exit zero prevents a second
            // destructive restart of the app.
            crate::runtime_error!("WORKER_RESULT failed");
            return;
        }
        eprintln!("❌ Error: {}", err);
        std::process::exit(1);
    } else if is_restart_worker {
        crate::runtime_print!("WORKER_RESULT passed");
    }
}
