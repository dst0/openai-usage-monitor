use super::cold_rollout_compressor::ColdRolloutCompressor;
use super::cold_rollout_scanner::ColdRolloutScanner;
use super::compression_checkpoints::CompressionCheckpoints;
use super::compression_outcome::CompressionOutcome;
use super::compression_stats::CompressionStats;
use super::rollout_file_name::RolloutFileName;
use super::rollout_maintenance_lock::RolloutMaintenanceLock;
use super::rollout_writer_locks::RolloutWriterLocks;
use std::collections::HashSet;
use std::io;
use std::path::PathBuf;
use std::time::{Duration, Instant, SystemTime};

/// One pass over a Codex home: scan, clean up after interrupted runs, and
/// compress every eligible rollout until the time budget runs out.
///
/// The whole pass holds Codex's rollout maintenance lock, so it never
/// overlaps Codex's own compressor or rollout migration.
pub struct RolloutCompressionService {
    codex_home: PathBuf,
    min_age: Duration,
    time_budget: Duration,
    /// Threads that must stay plain, such as recovery targets the Monitor
    /// still reads by byte offset.
    excluded_threads: HashSet<String>,
}

impl RolloutCompressionService {
    pub const DEFAULT_TIME_BUDGET: Duration = Duration::from_secs(30 * 60);

    pub fn new(codex_home: PathBuf, min_age: Duration, excluded_threads: HashSet<String>) -> Self {
        Self {
            codex_home,
            min_age,
            time_budget: Self::DEFAULT_TIME_BUDGET,
            excluded_threads: excluded_threads
                .into_iter()
                .map(|id| id.to_ascii_lowercase())
                .collect(),
        }
    }

    pub fn with_time_budget(mut self, time_budget: Duration) -> Self {
        self.time_budget = time_budget;
        self
    }

    /// Runs one pass. `dry_run` only counts what would be compressed.
    pub fn run<C: CompressionCheckpoints>(
        &self,
        checkpoints: &C,
        dry_run: bool,
    ) -> io::Result<CompressionStats> {
        let mut stats = CompressionStats::default();
        let Some(_maintenance) = RolloutMaintenanceLock::try_acquire(&self.codex_home)? else {
            stats.maintenance_busy = true;
            return Ok(stats);
        };
        let started = Instant::now();
        let now = SystemTime::now();
        let scan = ColdRolloutScanner::new(self.min_age, now).scan(&self.codex_home);
        stats.candidates = scan.candidates.len() as u64;
        stats.scan_errors = scan.errors;
        if !dry_run {
            for path in &scan.stale_staging {
                if std::fs::remove_file(path).is_ok() {
                    stats.stale_staging_removed += 1;
                }
            }
        }
        let locks = RolloutWriterLocks::new(&self.codex_home);
        let compressor = ColdRolloutCompressor::new(&locks, checkpoints, self.min_age, now);
        for path in &scan.candidates {
            if started.elapsed() >= self.time_budget {
                stats.time_budget_exhausted = true;
                break;
            }
            if self.is_excluded(path) {
                stats.record(CompressionOutcome::Excluded);
                continue;
            }
            if dry_run {
                stats.dry_run_bytes += std::fs::symlink_metadata(path).map_or(0, |m| m.len());
                continue;
            }
            match compressor.compress(path) {
                Ok(outcome) => stats.record(outcome),
                Err(_) => stats.failed += 1,
            }
        }
        Ok(stats)
    }

    fn is_excluded(&self, path: &std::path::Path) -> bool {
        RolloutFileName::from_path(path)
            .is_some_and(|name| self.excluded_threads.contains(name.thread_id()))
    }
}

#[cfg(test)]
#[path = "rollout_compression_service.test.rs"]
mod tests;
