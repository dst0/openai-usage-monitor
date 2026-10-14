use clap::Subcommand;

#[derive(Subcommand, Debug, Clone, PartialEq, Eq)]
pub(super) enum SessionsAction {
    /// Show how many Codex session rollouts are plain or compressed and the space saved
    Status,
    /// Compress idle session rollouts into the .jsonl.zst form Codex reads natively
    Compress {
        /// Only compress rollouts unchanged for at least this many days (default: the saved setting)
        #[arg(long, value_parser = clap::value_parser!(u64).range(1..=3650))]
        min_age_days: Option<u64>,
        /// Count what would be compressed without changing anything
        #[arg(long)]
        dry_run: bool,
        /// Internal: the daemon's low-priority daily pass
        #[arg(long, hide = true)]
        scheduled: bool,
    },
    /// Restore a compressed session to plain .jsonl for tools that cannot read zstd
    Decompress {
        /// Thread ID or codex://threads/<id> link
        thread_id: String,
    },
}
