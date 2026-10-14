//! Compresses cold Codex rollouts into the representation Codex itself reads.
//!
//! Codex 0.160 resolves `rollout-*.jsonl` and, when that is absent, the
//! `.jsonl.zst` beside it; it materializes the plain file again before the next
//! append. This module only produces that representation for idle rollouts,
//! under the same locks Codex's own compressor takes: the home-wide
//! `.tmp/rollout-maintenance.lock`, then per file the thread-writer
//! coordination lock with an idle probe of the thread's writer lock.

pub mod cold_rollout_compressor;
pub mod cold_rollout_scanner;
pub mod compressed_rollout_materializer;
pub mod compression_checkpoints;
pub mod compression_outcome;
pub mod compression_stats;
pub mod content_digest;
pub mod file_identity;
pub mod materialize_outcome;
pub mod no_compression_checkpoints;
pub mod rollout_compression_service;
pub mod rollout_file_name;
pub mod rollout_maintenance_lock;
pub mod rollout_scan;
pub mod rollout_storage_report;
pub mod rollout_writer_locks;
pub mod scheduled_compression_launcher;
pub mod session_compression_scheduler;
pub mod session_meta_reader;
pub mod sessions_command_service;
pub mod staging_file;
#[cfg(test)]
pub mod test_rollout_home;
pub mod zstd_rollout_codec;

/// Codex keeps rollouts in these two roots of its home.
pub const ROLLOUT_ROOTS: [&str; 2] = ["archived_sessions", "sessions"];
