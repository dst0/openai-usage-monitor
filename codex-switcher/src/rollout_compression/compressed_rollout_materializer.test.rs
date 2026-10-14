use super::CompressedRolloutMaterializer;
use crate::rollout_compression::materialize_outcome::MaterializeOutcome;
use crate::rollout_compression::no_compression_checkpoints::NoCompressionCheckpoints;
use crate::rollout_compression::rollout_compression_service::RolloutCompressionService;
use crate::rollout_compression::rollout_file_name::RolloutFileName;
use crate::rollout_compression::rollout_writer_locks::RolloutWriterLocks;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use fs2::FileExt;
use std::collections::HashSet;
use std::fs::File;
use std::time::Duration;

fn compressed_rollout(home: &TestRolloutHome, n: u32) -> (String, std::path::PathBuf, Vec<u8>) {
    let id = TestRolloutHome::thread_id(n);
    let bytes = TestRolloutHome::rollout_bytes(&id, 300);
    let plain = home.write_rollout("sessions", &id, &bytes, 30 * TestRolloutHome::DAY);
    RolloutCompressionService::new(home.path().to_path_buf(), 7 * TestRolloutHome::DAY, HashSet::new())
        .run(&NoCompressionCheckpoints, false)
        .unwrap();
    assert!(!plain.exists());
    (id, plain, bytes)
}

#[test]
fn restores_the_exact_bytes_and_modification_time() {
    let home = TestRolloutHome::new("restore-ok");
    let (id, plain, bytes) = compressed_rollout(&home, 1);
    let compressed = RolloutFileName::compressed_path(&plain);
    let modified = std::fs::metadata(&compressed).unwrap().modified().unwrap();
    let locks = RolloutWriterLocks::new(home.path());
    let materializer = CompressedRolloutMaterializer::new(home.path(), &locks);

    assert_eq!(materializer.find(&id.to_uppercase()), Some(plain.clone()));
    assert_eq!(
        materializer.restore(&plain).unwrap(),
        MaterializeOutcome::Restored { bytes: bytes.len() as u64 }
    );
    assert_eq!(std::fs::read(&plain).unwrap(), bytes);
    assert_eq!(std::fs::metadata(&plain).unwrap().modified().unwrap(), modified);
    assert!(!compressed.exists());
    assert_eq!(std::fs::read_dir(plain.parent().unwrap()).unwrap().count(), 1);
    assert_eq!(materializer.restore(&plain).unwrap(), MaterializeOutcome::AlreadyPlain);
}

#[test]
fn an_active_writer_blocks_restoration() {
    let home = TestRolloutHome::new("restore-busy");
    let (id, plain, _) = compressed_rollout(&home, 2);
    let lock_path = home.writer_lock_path(&id);
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let writer = File::create(&lock_path).unwrap();
    writer.lock_exclusive().unwrap();
    let locks = RolloutWriterLocks::with_coordination_wait(home.path(), Duration::from_millis(20));
    let outcome = CompressedRolloutMaterializer::new(home.path(), &locks).restore(&plain).unwrap();
    assert_eq!(outcome, MaterializeOutcome::Busy);
    assert!(!plain.exists());
    assert!(RolloutFileName::compressed_path(&plain).exists());
    assert_eq!(std::fs::read_dir(plain.parent().unwrap()).unwrap().count(), 1);
}

#[test]
fn unknown_threads_and_plain_rollouts_are_reported_without_changes() {
    let home = TestRolloutHome::new("restore-none");
    let id = TestRolloutHome::thread_id(3);
    let plain = home.write_rollout("sessions", &id, b"{}\n", TestRolloutHome::DAY);
    let locks = RolloutWriterLocks::new(home.path());
    let materializer = CompressedRolloutMaterializer::new(home.path(), &locks);
    assert_eq!(materializer.find(&TestRolloutHome::thread_id(4)), None);
    assert_eq!(materializer.find(&id), Some(plain.clone()));
    assert_eq!(materializer.restore(&plain).unwrap(), MaterializeOutcome::AlreadyPlain);
    std::fs::remove_file(&plain).unwrap();
    assert_eq!(materializer.restore(&plain).unwrap(), MaterializeOutcome::NotCompressed);
}
