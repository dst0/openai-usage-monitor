use super::compressed_rollout_materializer::CompressedRolloutMaterializer;
use super::materialize_outcome::MaterializeOutcome;
use super::no_compression_checkpoints::NoCompressionCheckpoints;
use super::rollout_compression_service::RolloutCompressionService;
use super::rollout_storage_report::{human, RolloutStorageReport};
use super::rollout_writer_locks::RolloutWriterLocks;
use std::collections::HashSet;
use std::time::Duration;

/// `cxi sessions …`: thin CLI coordination around the compression services.
pub struct SessionsCommandService;

const DAY_SECS: u64 = 24 * 60 * 60;

impl SessionsCommandService {
    pub fn status() -> Result<(), String> {
        let settings = crate::storage::load_accounts().unwrap_or_default().settings;
        for line in RolloutStorageReport::collect(&crate::storage::codex_home()).render() {
            println!("{line}");
        }
        println!(
            "Automatic compression: {} (rollouts idle for {} days)",
            if settings.session_compression_enabled { "on" } else { "off" },
            settings.session_compression_min_age_days
        );
        Ok(())
    }

    pub fn compress(min_age_days: Option<u64>, dry_run: bool, scheduled: bool) -> Result<(), String> {
        let settings = crate::storage::load_accounts().unwrap_or_default().settings;
        if scheduled {
            if !settings.session_compression_enabled {
                return Ok(());
            }
            lower_own_priority();
        }
        let days = min_age_days.unwrap_or(settings.session_compression_min_age_days).max(1);
        let excluded = recovery_targets()?;
        let stats = RolloutCompressionService::new(
            crate::storage::codex_home(),
            Duration::from_secs(days * DAY_SECS),
            excluded,
        )
        .run(&NoCompressionCheckpoints, dry_run)
        .map_err(|err| format!("Session compression failed: {err}"))?;
        let summary = stats.summary();
        if scheduled {
            crate::logger::log("INFO", "SESSION_COMPRESSION", &summary);
        } else if dry_run {
            println!(
                "Would compress {} rollouts ({}) idle for {days}+ days.",
                stats.candidates,
                human(stats.dry_run_bytes)
            );
        } else {
            println!(
                "Compressed {} rollouts: {} → {}.",
                stats.compressed,
                human(stats.source_bytes),
                human(stats.compressed_bytes)
            );
            println!("{summary}");
        }
        Ok(())
    }

    pub fn decompress(thread_id: &str) -> Result<(), String> {
        let id = crate::switcher::clean_thread_id(thread_id);
        if !crate::switcher::is_valid_thread_id(&id) {
            return Err(format!("Not a Codex thread ID: {thread_id}"));
        }
        let home = crate::storage::codex_home();
        let locks = RolloutWriterLocks::new(&home);
        let materializer = CompressedRolloutMaterializer::new(&home, &locks);
        let plain = materializer
            .find(&id)
            .ok_or_else(|| format!("No session rollout found for thread {id}"))?;
        let outcome = materializer
            .restore(&plain)
            .map_err(|err| format!("Could not restore {}: {err}", plain.display()))?;
        match outcome {
            MaterializeOutcome::Restored { bytes } => {
                println!("✅ Restored {} ({})", plain.display(), human(bytes))
            }
            MaterializeOutcome::AlreadyPlain => println!("Already plain: {}", plain.display()),
            MaterializeOutcome::NotCompressed => println!("Not compressed: {}", plain.display()),
            MaterializeOutcome::Busy => {
                return Err("Codex is writing this thread; try again when it is idle".into())
            }
            MaterializeOutcome::Changed => {
                return Err("The compressed rollout changed while it was read; try again".into())
            }
        }
        Ok(())
    }
}

/// Recovery reads its targets' plain rollouts by byte offset; they stay plain.
/// An unreadable manifest refuses the pass rather than guessing.
fn recovery_targets() -> Result<HashSet<String>, String> {
    let mut ids: HashSet<String> = crate::recovery::load_pending()?.into_iter().collect();
    ids.extend(crate::recovery::load_ownerless_pending()?);
    Ok(ids)
}

/// The scheduled pass yields CPU to everything interactive.
fn lower_own_priority() {
    // SAFETY: setpriority only changes this process's scheduling priority.
    unsafe {
        libc::setpriority(libc::PRIO_PROCESS, 0, 10);
    }
}
