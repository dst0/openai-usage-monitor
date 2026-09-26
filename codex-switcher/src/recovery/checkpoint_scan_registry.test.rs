use super::{checkpoint_scan_registry::CheckpointScanRegistry, pending_target::PendingTarget};
use std::{
    path::{Path, PathBuf},
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_HOME: AtomicU64 = AtomicU64::new(0);

const STARTED: &[u8] =
    b"checkpoint\n{\"type\":\"event_msg\",\"payload\":{\"type\":\"task_started\",\"turn_id\":\"new-turn\"}}\n";
const BUDGET: u64 = 16 * 1024 * 1024;

struct Checkpoint {
    target: PendingTarget,
    rollout: PathBuf,
}

/// A fresh directory per call, so no two tests (or two calls) share a home.
fn scratch_home(name: &str) -> PathBuf {
    let home = std::env::temp_dir().join(format!(
        "codex-scan-registry-test-{name}-{}-{}",
        std::process::id(),
        NEXT_HOME.fetch_add(1, Ordering::Relaxed)
    ));
    std::fs::create_dir(&home).expect("scratch home must be new");
    std::fs::create_dir(home.join("sessions")).unwrap();
    home
}

/// A rollout whose checkpoint is followed by one started turn, so a complete
/// probe reports `(started, not verified)`.
fn checkpoint(home: &Path, index: u64) -> Checkpoint {
    let id = format!("01a098c2-0fae-74d2-a80c-{:012x}", 0xe000 + index);
    let rollout = home
        .join("sessions")
        .join(format!("rollout-2026-09-26T00-00-00-{id}.jsonl"));
    std::fs::write(&rollout, STARTED).unwrap();
    Checkpoint {
        target: PendingTarget {
            id,
            offset: Some(11),
            awaiting_owner: true,
            captured_restart: true,
            owner_account_id: Some("old-account".into()),
        },
        rollout,
    }
}

fn probe(scans: &CheckpointScanRegistry, home: &Path, checkpoint: &Checkpoint) {
    let mut budget = BUDGET;
    assert_eq!(
        scans.post_checkpoint_status_with_budget(home, &checkpoint.target, &mut budget),
        Some((true, false))
    );
}

/// Scans at most `budget` bytes. `scanned_bytes` accumulates for as long as
/// one cached cursor lives, so a reused cursor keeps counting while a rebuilt
/// one starts again from zero.
fn probe_partially(scans: &CheckpointScanRegistry, home: &Path, checkpoint: &Checkpoint) {
    let mut budget = 8;
    assert_eq!(
        scans.post_checkpoint_status_with_budget(home, &checkpoint.target, &mut budget),
        None,
        "an incomplete scan reports no evidence"
    );
    assert_eq!(budget, 0);
}

fn cached(scans: &CheckpointScanRegistry, checkpoint: &Checkpoint) -> Option<u64> {
    scans.scanned_bytes_for(&checkpoint.rollout, 11)
}

fn confirm(scans: &CheckpointScanRegistry, home: &Path, checkpoint: &Checkpoint) {
    let mut budget = BUDGET;
    assert_eq!(
        scans.confirmed_checkpoint_status_with_budget(home, &checkpoint.target, &mut budget),
        Some((true, false))
    );
}

fn confirmed(scans: &CheckpointScanRegistry, checkpoint: &Checkpoint) -> Option<u64> {
    scans.confirmation_cursor_for(&checkpoint.rollout, 11)
}

#[test]
fn full_registry_evicts_the_least_recently_probed_checkpoint() {
    let home = scratch_home("lru");
    let scans = CheckpointScanRegistry::new(2);
    let [first, second, third] = [1, 2, 3].map(|index| checkpoint(&home, index));
    probe_partially(&scans, &home, &first);
    probe_partially(&scans, &home, &second);
    // Probing `first` again reuses its cursor and makes `second` the least
    // recently probed entry.
    probe_partially(&scans, &home, &first);
    assert_eq!(cached(&scans, &first), Some(16));
    probe_partially(&scans, &home, &third);

    assert_eq!(cached(&scans, &second), None);
    assert_eq!(cached(&scans, &third), Some(8));
    // The retained cursor keeps accumulating instead of starting over, and
    // the evicted checkpoint is rebuilt from its offset.
    probe_partially(&scans, &home, &first);
    assert_eq!(cached(&scans, &first), Some(24));
    probe_partially(&scans, &home, &second);
    assert_eq!(cached(&scans, &second), Some(8));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn full_registry_evicts_the_least_recently_probed_confirmation() {
    let home = scratch_home("confirm-lru");
    let scans = CheckpointScanRegistry::new(2);
    let [first, second, third] = [1, 2, 3].map(|index| checkpoint(&home, index));
    // Two append cursors first: filling the confirmation cache must not
    // touch them.
    probe_partially(&scans, &home, &first);
    probe_partially(&scans, &home, &second);
    confirm(&scans, &home, &first);
    confirm(&scans, &home, &second);
    confirm(&scans, &home, &first);
    confirm(&scans, &home, &third);

    assert_eq!(confirmed(&scans, &second), None);
    assert!(confirmed(&scans, &first).is_some());
    assert!(confirmed(&scans, &third).is_some());
    assert!(scans.confirmed_snapshot_still_current(&home, &first.target));
    assert!(!scans.confirmed_snapshot_still_current(&home, &second.target));
    assert_eq!(cached(&scans, &first), Some(8));
    assert_eq!(cached(&scans, &second), Some(8));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn single_entry_registry_holds_only_the_latest_checkpoint() {
    let home = scratch_home("single");
    let scans = CheckpointScanRegistry::new(1);
    let [first, second] = [1, 2].map(|index| checkpoint(&home, index));
    probe(&scans, &home, &first);
    probe(&scans, &home, &second);
    assert_eq!(cached(&scans, &first), None);
    assert!(cached(&scans, &second).is_some());
    // The evicted checkpoint is rescanned from its offset, not lost.
    probe(&scans, &home, &first);
    assert!(cached(&scans, &first).is_some());
    assert_eq!(cached(&scans, &second), None);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn replaced_rollout_reuses_its_slot_without_evicting_another_checkpoint() {
    let home = scratch_home("replace");
    let scans = CheckpointScanRegistry::new(2);
    let [first, second] = [1, 2].map(|index| checkpoint(&home, index));
    probe(&scans, &home, &first);
    probe(&scans, &home, &second);
    // A new inode cannot continue the cached cursor, so the entry is rebuilt.
    std::fs::remove_file(&first.rollout).unwrap();
    std::fs::write(&first.rollout, b"checkpoint\n").unwrap();
    let mut budget = BUDGET;
    assert_eq!(
        scans.post_checkpoint_status_with_budget(&home, &first.target, &mut budget),
        Some((false, false))
    );
    assert!(cached(&scans, &second).is_some());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn separate_registries_do_not_share_or_evict_entries() {
    let home = scratch_home("isolated");
    let mine = CheckpointScanRegistry::new(1);
    let other = CheckpointScanRegistry::new(1);
    let [first, second] = [1, 2].map(|index| checkpoint(&home, index));
    probe(&mine, &home, &first);
    probe(&other, &home, &second);
    confirm(&other, &home, &second);
    assert!(cached(&mine, &first).is_some());
    assert_eq!(cached(&mine, &second), None);
    assert_eq!(cached(&other, &first), None);
    assert_eq!(confirmed(&mine, &second), None);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
#[should_panic(expected = "at least one entry")]
fn zero_capacity_registry_is_rejected() {
    // Eviction removes the oldest entry before inserting, which needs room
    // for at least the probed checkpoint itself.
    let _ = CheckpointScanRegistry::new(std::hint::black_box(0));
}

fn append_oversized_turn(rollout: &Path) {
    // A turn ID longer than the cache bound makes the scan fail.
    let oversized = format!(
        "{{\"type\":\"event_msg\",\"payload\":{{\"type\":\"task_started\",\"turn_id\":\"{}\"}}}}\n",
        "t".repeat(300)
    );
    let mut file = std::fs::OpenOptions::new()
        .append(true)
        .open(rollout)
        .unwrap();
    std::io::Write::write_all(&mut file, oversized.as_bytes()).unwrap();
}

#[test]
fn failed_probe_drops_its_cursor_instead_of_keeping_partial_evidence() {
    let home = scratch_home("failed-probe");
    let scans = CheckpointScanRegistry::new(2);
    let [first, second] = [1, 2].map(|index| checkpoint(&home, index));
    probe(&scans, &home, &first);
    probe(&scans, &home, &second);
    append_oversized_turn(&first.rollout);
    let mut budget = BUDGET;
    assert_eq!(
        scans.post_checkpoint_status_with_budget(&home, &first.target, &mut budget),
        None
    );
    assert_eq!(cached(&scans, &first), None);
    assert!(cached(&scans, &second).is_some());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_first_probe_does_not_evict_a_cached_checkpoint() {
    let home = scratch_home("failed-new-probe");
    let scans = CheckpointScanRegistry::new(2);
    let [first, second, third] = [1, 2, 3].map(|index| checkpoint(&home, index));
    probe(&scans, &home, &first);
    probe(&scans, &home, &second);
    append_oversized_turn(&third.rollout);
    let mut budget = BUDGET;
    assert_eq!(
        scans.post_checkpoint_status_with_budget(&home, &third.target, &mut budget),
        None
    );
    // The full registry evicts only after a new scan succeeds, so a probe
    // that fails costs no other checkpoint its progress.
    assert_eq!(cached(&scans, &third), None);
    assert!(cached(&scans, &first).is_some());
    assert!(cached(&scans, &second).is_some());
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_first_confirmation_does_not_evict_a_cached_confirmation() {
    let home = scratch_home("failed-new-confirmation");
    let scans = CheckpointScanRegistry::new(2);
    let [first, second, third] = [1, 2, 3].map(|index| checkpoint(&home, index));
    confirm(&scans, &home, &first);
    confirm(&scans, &home, &second);
    append_oversized_turn(&third.rollout);
    for _ in 0..2 {
        let mut budget = BUDGET;
        assert_eq!(
            scans.confirmed_checkpoint_status_with_budget(&home, &third.target, &mut budget),
            None
        );
        // The failed scan is dropped rather than cached, so a retry scans
        // again instead of holding a slot that proves nothing.
        assert_eq!(confirmed(&scans, &third), None);
    }
    assert!(scans.confirmed_snapshot_still_current(&home, &first.target));
    assert!(scans.confirmed_snapshot_still_current(&home, &second.target));
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn changed_snapshot_restarts_its_confirmation() {
    let home = scratch_home("changed-confirmation");
    let scans = CheckpointScanRegistry::new(2);
    let first = checkpoint(&home, 1);
    confirm(&scans, &home, &first);
    assert!(scans.confirmed_snapshot_still_current(&home, &first.target));
    let mut rollout = std::fs::OpenOptions::new()
        .append(true)
        .open(&first.rollout)
        .unwrap();
    std::io::Write::write_all(
        &mut rollout,
        b"{\"type\":\"event_msg\",\"payload\":{\"type\":\"token_count\"}}\n",
    )
    .unwrap();
    drop(rollout);

    // An append invalidates the pinned snapshot; the proof starts over from
    // the checkpoint rather than trusting the earlier result.
    assert!(!scans.confirmed_snapshot_still_current(&home, &first.target));
    let mut budget = BUDGET;
    assert_eq!(
        scans.confirmed_checkpoint_status_with_budget(&home, &first.target, &mut budget),
        Some((true, false))
    );
    assert_eq!(
        confirmed(&scans, &first),
        Some(std::fs::metadata(&first.rollout).unwrap().len())
    );
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn partial_confirmation_is_not_a_current_snapshot() {
    let home = scratch_home("partial-confirmation");
    let scans = CheckpointScanRegistry::new(2);
    let first = checkpoint(&home, 1);
    // A budget smaller than the post-checkpoint interval leaves the pinned
    // scan incomplete, even though the rollout itself has not changed.
    let mut budget = 8;
    assert_eq!(
        scans.confirmed_checkpoint_status_with_budget(&home, &first.target, &mut budget),
        None
    );
    assert_eq!(budget, 0);
    assert!(!scans.confirmed_snapshot_still_current(&home, &first.target));

    confirm(&scans, &home, &first);
    assert!(scans.confirmed_snapshot_still_current(&home, &first.target));
    std::fs::remove_dir_all(home).unwrap();
}
