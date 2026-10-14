use super::{human, RolloutStorageReport};
use crate::rollout_compression::no_compression_checkpoints::NoCompressionCheckpoints;
use crate::rollout_compression::rollout_compression_service::RolloutCompressionService;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use std::collections::HashSet;

#[test]
fn counts_plain_and_compressed_rollouts_with_their_original_size() {
    let home = TestRolloutHome::new("report");
    let (cold, warm) = (TestRolloutHome::thread_id(1), TestRolloutHome::thread_id(2));
    let cold_bytes = TestRolloutHome::rollout_bytes(&cold, 400);
    let warm_bytes = TestRolloutHome::rollout_bytes(&warm, 10);
    home.write_rollout("sessions", &cold, &cold_bytes, 30 * TestRolloutHome::DAY);
    home.write_rollout("archived_sessions", &warm, &warm_bytes, TestRolloutHome::DAY);
    std::fs::write(home.path().join("sessions/notes.txt"), b"ignored").unwrap();
    RolloutCompressionService::new(home.path().to_path_buf(), 7 * TestRolloutHome::DAY, HashSet::new())
        .run(&NoCompressionCheckpoints, false)
        .unwrap();

    let report = RolloutStorageReport::collect(home.path());

    assert_eq!((report.plain_files, report.plain_bytes), (1, warm_bytes.len() as u64));
    assert_eq!(report.compressed_files, 1);
    assert_eq!(report.compressed_original_bytes, cold_bytes.len() as u64);
    assert!(report.compressed_bytes > 0 && report.compressed_bytes < cold_bytes.len() as u64);
    assert!(report.render().iter().any(|line| line.starts_with("Saved:")));
}

#[test]
fn human_sizes_use_binary_units() {
    assert_eq!(human(0), "0 B");
    assert_eq!(human(1023), "1023 B");
    assert_eq!(human(1536), "1.5 KiB");
    assert_eq!(human(5 * 1024 * 1024 * 1024), "5.0 GiB");
}
