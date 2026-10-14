use super::RolloutCompressionService;
use crate::rollout_compression::no_compression_checkpoints::NoCompressionCheckpoints;
use crate::rollout_compression::rollout_file_name::RolloutFileName;
use crate::rollout_compression::rollout_maintenance_lock::RolloutMaintenanceLock;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use std::collections::HashSet;
use std::time::Duration;

const DAY: Duration = TestRolloutHome::DAY;

fn service(home: &TestRolloutHome, excluded: &[&str]) -> RolloutCompressionService {
    RolloutCompressionService::new(
        home.path().to_path_buf(),
        7 * DAY,
        excluded.iter().map(|id| id.to_string()).collect::<HashSet<_>>(),
    )
}

#[test]
fn compresses_every_cold_rollout_and_keeps_warm_and_excluded_ones() {
    let home = TestRolloutHome::new("service-pass");
    let ids: Vec<String> = (1..=4).map(TestRolloutHome::thread_id).collect();
    let cold_a = home.write_rollout("sessions", &ids[0], &TestRolloutHome::rollout_bytes(&ids[0], 50), 9 * DAY);
    let cold_b = home.write_rollout("archived_sessions", &ids[1], &TestRolloutHome::rollout_bytes(&ids[1], 50), 30 * DAY);
    let warm = home.write_rollout("sessions", &ids[2], &TestRolloutHome::rollout_bytes(&ids[2], 50), DAY);
    let excluded = home.write_rollout("sessions", &ids[3], &TestRolloutHome::rollout_bytes(&ids[3], 50), 9 * DAY);

    let stats = service(&home, &[&ids[3].to_uppercase()]).run(&NoCompressionCheckpoints, false).unwrap();

    assert_eq!(stats.candidates, 3);
    assert_eq!(stats.compressed, 2);
    assert_eq!(stats.skipped.get("excluded"), Some(&1));
    assert_eq!(stats.failed, 0);
    assert!(stats.compressed_bytes < stats.source_bytes);
    for path in [&cold_a, &cold_b] {
        assert!(!path.exists() && RolloutFileName::compressed_path(path).exists());
    }
    for path in [&warm, &excluded] {
        assert!(path.exists() && !RolloutFileName::compressed_path(path).exists());
    }
    // A second pass finds nothing left to do.
    let again = service(&home, &[&ids[3]]).run(&NoCompressionCheckpoints, false).unwrap();
    assert_eq!((again.candidates, again.compressed), (1, 0));
}

#[test]
fn dry_run_changes_nothing() {
    let home = TestRolloutHome::new("service-dry");
    let id = TestRolloutHome::thread_id(5);
    let bytes = TestRolloutHome::rollout_bytes(&id, 20);
    let path = home.write_rollout("sessions", &id, &bytes, 9 * DAY);
    let stale = path.parent().unwrap().join(".rollout-z.jsonl.zst.cxi-0123456789abcdef.tmp");
    std::fs::write(&stale, b"x").unwrap();
    TestRolloutHome::set_age(&stale, 2 * DAY);

    let stats = service(&home, &[]).run(&NoCompressionCheckpoints, true).unwrap();

    assert_eq!((stats.candidates, stats.compressed), (1, 0));
    assert_eq!(stats.dry_run_bytes, bytes.len() as u64);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(stale.exists());
}

#[test]
fn removes_stale_staging_left_by_an_interrupted_run() {
    let home = TestRolloutHome::new("service-stale");
    let directory = home.path().join("sessions/2026/01/02");
    std::fs::create_dir_all(&directory).unwrap();
    let stale = directory.join(".rollout-z.jsonl.zst.cxi-0123456789abcdef.tmp");
    std::fs::write(&stale, b"x").unwrap();
    TestRolloutHome::set_age(&stale, 2 * DAY);
    let stats = service(&home, &[]).run(&NoCompressionCheckpoints, false).unwrap();
    assert_eq!(stats.stale_staging_removed, 1);
    assert!(!stale.exists());
}

#[test]
fn yields_to_another_maintenance_job() {
    let home = TestRolloutHome::new("service-busy");
    let id = TestRolloutHome::thread_id(6);
    let path = home.write_rollout("sessions", &id, &TestRolloutHome::rollout_bytes(&id, 5), 9 * DAY);
    let _codex_compressor = RolloutMaintenanceLock::try_acquire(home.path()).unwrap().unwrap();
    let stats = service(&home, &[]).run(&NoCompressionCheckpoints, false).unwrap();
    assert!(stats.maintenance_busy);
    assert_eq!(stats.compressed, 0);
    assert!(path.exists());
}

#[test]
fn stops_at_the_time_budget() {
    let home = TestRolloutHome::new("service-budget");
    for n in 7..=9 {
        let id = TestRolloutHome::thread_id(n);
        home.write_rollout("sessions", &id, &TestRolloutHome::rollout_bytes(&id, 5), 9 * DAY);
    }
    let stats = service(&home, &[])
        .with_time_budget(Duration::ZERO)
        .run(&NoCompressionCheckpoints, false)
        .unwrap();
    assert!(stats.time_budget_exhausted);
    assert_eq!(stats.compressed, 0);
}
