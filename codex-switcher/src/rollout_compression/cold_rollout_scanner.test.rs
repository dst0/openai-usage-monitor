use super::ColdRolloutScanner;
use crate::rollout_compression::rollout_file_name::RolloutFileName;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use std::time::SystemTime;

const DAY: std::time::Duration = TestRolloutHome::DAY;

#[test]
fn finds_cold_plain_rollouts_in_both_roots_oldest_first() {
    let home = TestRolloutHome::new("scan-roots");
    let (a, b, c) = (
        TestRolloutHome::thread_id(1),
        TestRolloutHome::thread_id(2),
        TestRolloutHome::thread_id(3),
    );
    let newer = home.write_rollout("sessions", &a, b"{}\n", 10 * DAY);
    let older = home.write_rollout("archived_sessions", &b, b"{}\n", 20 * DAY);
    home.write_rollout("sessions", &c, b"{}\n", DAY / 2);
    let scan = ColdRolloutScanner::new(7 * DAY, SystemTime::now()).scan(home.path());
    assert_eq!(scan.candidates, vec![older, newer]);
    assert_eq!(scan.errors, 0);
}

#[test]
fn skips_compressed_siblings_foreign_files_and_symlinks() {
    let home = TestRolloutHome::new("scan-skip");
    let thread = TestRolloutHome::thread_id(4);
    let plain = home.write_rollout("sessions", &thread, b"{}\n", 30 * DAY);
    std::fs::write(RolloutFileName::compressed_path(&plain), b"frame").unwrap();
    let directory = plain.parent().unwrap();
    std::fs::write(directory.join("notes.jsonl"), b"x").unwrap();
    let other = TestRolloutHome::thread_id(5);
    let target = home.write_rollout("archived_sessions", &other, b"{}\n", 30 * DAY);
    let linked_dir = home.path().join("sessions/linked");
    std::os::unix::fs::symlink(target.parent().unwrap(), &linked_dir).unwrap();
    std::os::unix::fs::symlink(&target, directory.join(target.file_name().unwrap())).unwrap();

    let scan = ColdRolloutScanner::new(7 * DAY, SystemTime::now()).scan(home.path());
    assert_eq!(scan.candidates, vec![target], "only the real file, found once");
}

#[test]
fn reports_only_old_staging_files_of_our_own_shape() {
    let home = TestRolloutHome::new("scan-staging");
    let directory = home.path().join("sessions/2026/01/02");
    std::fs::create_dir_all(&directory).unwrap();
    let old = directory.join(".rollout-x.jsonl.zst.cxi-0123456789abcdef.tmp");
    let fresh = directory.join(".rollout-y.jsonl.zst.cxi-fedcba9876543210.tmp");
    let codex = directory.join("rollout-compress-abc.tmp");
    for path in [&old, &fresh, &codex] {
        std::fs::write(path, b"partial").unwrap();
    }
    TestRolloutHome::set_age(&old, 2 * DAY);
    TestRolloutHome::set_age(&codex, 2 * DAY);
    let scan = ColdRolloutScanner::new(7 * DAY, SystemTime::now()).scan(home.path());
    assert_eq!(scan.stale_staging, vec![old]);
    assert!(scan.candidates.is_empty());
}

#[test]
fn a_missing_home_is_an_empty_scan_not_an_error() {
    let home = TestRolloutHome::new("scan-empty");
    let scan = ColdRolloutScanner::new(DAY, SystemTime::now()).scan(&home.path().join("absent"));
    assert!(scan.candidates.is_empty());
    assert_eq!(scan.errors, 0);
}
