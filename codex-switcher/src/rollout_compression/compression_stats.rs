use super::compression_outcome::CompressionOutcome;
use std::collections::BTreeMap;

/// Totals of one compression pass.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct CompressionStats {
    pub candidates: u64,
    pub compressed: u64,
    pub source_bytes: u64,
    pub compressed_bytes: u64,
    /// Skipped files by outcome label.
    pub skipped: BTreeMap<&'static str, u64>,
    pub failed: u64,
    pub scan_errors: u64,
    pub stale_staging_removed: u64,
    /// Another maintenance job (Codex's compressor or migration) was running.
    pub maintenance_busy: bool,
    pub time_budget_exhausted: bool,
    /// Bytes that a dry run would compress.
    pub dry_run_bytes: u64,
}

impl CompressionStats {
    pub fn record(&mut self, outcome: CompressionOutcome) {
        match outcome {
            CompressionOutcome::Compressed {
                source_bytes,
                compressed_bytes,
            } => {
                self.compressed += 1;
                self.source_bytes += source_bytes;
                self.compressed_bytes += compressed_bytes;
            }
            other => *self.skipped.entry(other.label()).or_default() += 1,
        }
    }

    /// One line for logs and the CLI.
    pub fn summary(&self) -> String {
        if self.maintenance_busy {
            return "skipped: another rollout maintenance job is running".to_string();
        }
        let skipped: Vec<String> = self
            .skipped
            .iter()
            .map(|(label, count)| format!("{label}={count}"))
            .collect();
        format!(
            "candidates={} compressed={} {}→{} bytes failed={} scan_errors={} stale_staging_removed={} skipped=[{}]{}",
            self.candidates,
            self.compressed,
            self.source_bytes,
            self.compressed_bytes,
            self.failed,
            self.scan_errors,
            self.stale_staging_removed,
            skipped.join(" "),
            if self.time_budget_exhausted { " (time budget reached)" } else { "" }
        )
    }
}
