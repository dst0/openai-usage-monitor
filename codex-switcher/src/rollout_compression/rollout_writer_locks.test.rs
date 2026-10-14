use super::RolloutWriterLocks;
use crate::rollout_compression::rollout_maintenance_lock::RolloutMaintenanceLock;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use fs2::FileExt;
use std::fs::OpenOptions;
use std::time::{Duration, Instant};

#[test]
fn a_thread_without_a_lock_file_is_idle_and_the_probe_creates_none() {
    let home = TestRolloutHome::new("writer-idle");
    let locks = RolloutWriterLocks::new(home.path());
    let thread = TestRolloutHome::thread_id(1);
    let _coordination = locks.lock_coordination().unwrap().expect("free");
    assert!(locks.thread_is_idle(&thread).unwrap());
    assert!(!home.writer_lock_path(&thread).exists());
}

#[test]
fn a_held_writer_lock_means_busy_and_a_released_one_means_idle() {
    let home = TestRolloutHome::new("writer-held");
    let locks = RolloutWriterLocks::new(home.path());
    let thread = TestRolloutHome::thread_id(2);
    let _coordination = locks.lock_coordination().unwrap().expect("free");
    let writer = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(home.writer_lock_path(&thread))
        .unwrap();
    writer.lock_exclusive().unwrap();
    assert!(!locks.thread_is_idle(&thread).unwrap());
    // The probe must not have stolen or kept the writer's lock.
    assert!(!locks.thread_is_idle(&thread).unwrap());
    writer.unlock().unwrap();
    assert!(locks.thread_is_idle(&thread).unwrap());
    // And the idle probe released what it took.
    writer.try_lock_exclusive().expect("probe left the lock free");
}

#[test]
fn coordination_is_exclusive_and_waits_only_as_long_as_configured() {
    let home = TestRolloutHome::new("coordination");
    let held = RolloutWriterLocks::new(home.path())
        .lock_coordination()
        .unwrap()
        .expect("free");
    let waiter = RolloutWriterLocks::with_coordination_wait(home.path(), Duration::from_millis(60));
    let started = Instant::now();
    assert!(waiter.lock_coordination().unwrap().is_none());
    assert!(started.elapsed() >= Duration::from_millis(60));
    drop(held);
    assert!(waiter.lock_coordination().unwrap().is_some());
}

#[test]
fn maintenance_lock_excludes_a_second_holder_until_released() {
    let home = TestRolloutHome::new("maintenance");
    let first = RolloutMaintenanceLock::try_acquire(home.path())
        .unwrap()
        .expect("free");
    assert!(RolloutMaintenanceLock::try_acquire(home.path())
        .unwrap()
        .is_none());
    drop(first);
    assert!(RolloutMaintenanceLock::try_acquire(home.path())
        .unwrap()
        .is_some());
    assert!(home.path().join(".tmp/rollout-maintenance.lock").is_file());
}
