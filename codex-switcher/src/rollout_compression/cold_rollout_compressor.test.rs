use super::ColdRolloutCompressor;
use crate::rollout_compression::compression_checkpoints::CompressionCheckpoints;
use crate::rollout_compression::compression_outcome::CompressionOutcome;
use crate::rollout_compression::no_compression_checkpoints::NoCompressionCheckpoints;
use crate::rollout_compression::rollout_file_name::RolloutFileName;
use crate::rollout_compression::rollout_writer_locks::RolloutWriterLocks;
use crate::rollout_compression::test_rollout_home::TestRolloutHome;
use fs2::FileExt;
use std::fs::{File, OpenOptions};
use std::io::Write;
use std::os::unix::fs::{MetadataExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

const MIN_AGE: Duration = TestRolloutHome::DAY;

struct Hooks<A: Fn(&Path), B: Fn(&Path)> {
    after_encode: A,
    before_remove: B,
}

impl<A: Fn(&Path), B: Fn(&Path)> CompressionCheckpoints for Hooks<A, B> {
    fn after_encode(&self, source: &Path) {
        (self.after_encode)(source)
    }
    fn before_remove_source(&self, source: &Path) {
        (self.before_remove)(source)
    }
}

fn compress_with<C: CompressionCheckpoints>(home: &TestRolloutHome, path: &Path, hooks: &C) -> CompressionOutcome {
    let locks = RolloutWriterLocks::with_coordination_wait(home.path(), Duration::from_millis(50));
    ColdRolloutCompressor::new(&locks, hooks, MIN_AGE, SystemTime::now())
        .compress(path)
        .expect("compression must not fail")
}

fn compress(home: &TestRolloutHome, path: &Path) -> CompressionOutcome {
    compress_with(home, path, &NoCompressionCheckpoints)
}

fn cold_rollout(home: &TestRolloutHome, n: u32) -> (PathBuf, Vec<u8>) {
    let thread = TestRolloutHome::thread_id(n);
    let bytes = TestRolloutHome::rollout_bytes(&thread, 500);
    let path = home.write_rollout("sessions", &thread, &bytes, 3 * MIN_AGE);
    (path, bytes)
}

fn entries(directory: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    names.sort();
    names
}

fn append(path: &Path, text: &str) {
    OpenOptions::new().append(true).open(path).unwrap().write_all(text.as_bytes()).unwrap();
}

#[test]
fn cold_idle_rollout_becomes_a_byte_identical_codex_frame() {
    let home = TestRolloutHome::new("compress-ok");
    let (path, bytes) = cold_rollout(&home, 1);
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o640)).unwrap();
    let before = std::fs::metadata(&path).unwrap();

    let outcome = compress(&home, &path);

    let compressed = RolloutFileName::compressed_path(&path);
    let after = std::fs::metadata(&compressed).unwrap();
    assert_eq!(
        outcome,
        CompressionOutcome::Compressed { source_bytes: bytes.len() as u64, compressed_bytes: after.len() }
    );
    assert!(!path.exists(), "plain rollout must be removed");
    assert_eq!(zstd::stream::decode_all(File::open(&compressed).unwrap()).unwrap(), bytes);
    assert_eq!(after.modified().unwrap(), before.modified().unwrap());
    assert_eq!(after.mode() & 0o7777, 0o640);
    assert_eq!(entries(path.parent().unwrap()), vec![compressed.file_name().unwrap().to_str().unwrap()]);
}

#[test]
fn recently_modified_rollout_is_left_alone() {
    let home = TestRolloutHome::new("compress-warm");
    let thread = TestRolloutHome::thread_id(2);
    let bytes = TestRolloutHome::rollout_bytes(&thread, 5);
    let path = home.write_rollout("sessions", &thread, &bytes, MIN_AGE / 2);
    assert_eq!(compress(&home, &path), CompressionOutcome::NotCold);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(entries(path.parent().unwrap()).len(), 1);
}

#[test]
fn an_existing_compressed_sibling_is_never_replaced() {
    let home = TestRolloutHome::new("compress-sibling");
    let (path, bytes) = cold_rollout(&home, 3);
    let compressed = RolloutFileName::compressed_path(&path);
    std::fs::write(&compressed, b"older frame").unwrap();
    assert_eq!(compress(&home, &path), CompressionOutcome::AlreadyCompressed);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(std::fs::read(&compressed).unwrap(), b"older frame");
}

#[test]
fn a_thread_with_an_active_writer_is_skipped_untouched() {
    let home = TestRolloutHome::new("compress-busy");
    let (path, bytes) = cold_rollout(&home, 4);
    let lock_path = home.writer_lock_path(&TestRolloutHome::thread_id(4));
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let writer = File::create(&lock_path).unwrap();
    writer.lock_exclusive().unwrap();
    assert_eq!(compress(&home, &path), CompressionOutcome::Busy);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(entries(path.parent().unwrap()).len(), 1, "staging must be removed");
}

#[test]
fn held_writer_coordination_skips_the_file_after_the_bounded_wait() {
    let home = TestRolloutHome::new("compress-coordination");
    let (path, bytes) = cold_rollout(&home, 5);
    let _held = RolloutWriterLocks::new(home.path()).lock_coordination().unwrap().unwrap();
    assert_eq!(compress(&home, &path), CompressionOutcome::Busy);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(entries(path.parent().unwrap()).len(), 1);
}

#[test]
fn reverted_rollout_name_takes_the_lock_of_its_thread_not_its_rollout() {
    let home = TestRolloutHome::new("compress-reverted");
    let thread = TestRolloutHome::thread_id(6);
    let rollout = TestRolloutHome::thread_id(60);
    let directory = home.path().join("sessions/2026/01/02");
    std::fs::create_dir_all(&directory).unwrap();
    let path = directory.join(format!("rollout-2026-01-02T03-04-05-{thread}_{rollout}.jsonl"));
    std::fs::write(&path, TestRolloutHome::rollout_bytes(&thread, 3)).unwrap();
    TestRolloutHome::set_age(&path, 3 * MIN_AGE);
    let lock_path = home.writer_lock_path(&thread);
    std::fs::create_dir_all(lock_path.parent().unwrap()).unwrap();
    let writer = File::create(&lock_path).unwrap();
    writer.lock_exclusive().unwrap();
    assert_eq!(compress(&home, &path), CompressionOutcome::Busy);
    writer.unlock().unwrap();
    assert!(matches!(compress(&home, &path), CompressionOutcome::Compressed { .. }));
}

#[test]
fn content_that_names_another_thread_is_not_compressed() {
    let home = TestRolloutHome::new("compress-mismatch");
    let thread = TestRolloutHome::thread_id(7);
    let bytes = TestRolloutHome::rollout_bytes(&TestRolloutHome::thread_id(8), 3);
    let path = home.write_rollout("sessions", &thread, &bytes, 3 * MIN_AGE);
    assert_eq!(compress(&home, &path), CompressionOutcome::MetadataMismatch);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn symlinks_and_foreign_names_are_not_rollouts() {
    let home = TestRolloutHome::new("compress-symlink");
    let (target, bytes) = cold_rollout(&home, 9);
    let link_dir = home.path().join("archived_sessions");
    std::fs::create_dir_all(&link_dir).unwrap();
    let link = link_dir.join(target.file_name().unwrap());
    std::os::unix::fs::symlink(&target, &link).unwrap();
    assert_eq!(compress(&home, &link), CompressionOutcome::NotRollout);
    let foreign = home.path().join("sessions/notes.jsonl");
    std::fs::write(&foreign, b"x\n").unwrap();
    assert_eq!(compress(&home, &foreign), CompressionOutcome::NotRollout);
    assert_eq!(std::fs::read(&target).unwrap(), bytes);
    assert!(!RolloutFileName::compressed_path(&link).exists());
}

#[test]
fn an_append_during_encoding_keeps_the_plain_file_and_publishes_nothing() {
    let home = TestRolloutHome::new("compress-append");
    let (path, mut bytes) = cold_rollout(&home, 10);
    let hooks = Hooks { after_encode: |source: &Path| append(source, "{\"late\":1}\n"), before_remove: |_: &Path| {} };
    assert_eq!(compress_with(&home, &path, &hooks), CompressionOutcome::Changed);
    bytes.extend_from_slice(b"{\"late\":1}\n");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert_eq!(entries(path.parent().unwrap()).len(), 1);
}

#[test]
fn a_change_after_publication_withdraws_the_frame() {
    let home = TestRolloutHome::new("compress-late-change");
    let (path, mut bytes) = cold_rollout(&home, 11);
    let hooks = Hooks { after_encode: |_: &Path| {}, before_remove: |source: &Path| append(source, "{\"late\":2}\n") };
    assert_eq!(compress_with(&home, &path, &hooks), CompressionOutcome::Changed);
    bytes.extend_from_slice(b"{\"late\":2}\n");
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
    assert!(!RolloutFileName::compressed_path(&path).exists());
}

#[test]
fn a_rollout_replaced_by_rename_with_the_same_metadata_is_detected() {
    let home = TestRolloutHome::new("compress-replaced");
    let (path, bytes) = cold_rollout(&home, 12);
    let modified = std::fs::metadata(&path).unwrap().modified().unwrap();
    let hooks = Hooks {
        after_encode: |source: &Path| {
            let other = source.with_extension("other");
            std::fs::write(&other, &bytes).unwrap();
            File::options().write(true).open(&other).unwrap()
                .set_times(std::fs::FileTimes::new().set_modified(modified)).unwrap();
            std::fs::rename(&other, source).unwrap();
        },
        before_remove: |_: &Path| {},
    };
    assert_eq!(compress_with(&home, &path, &hooks), CompressionOutcome::Changed);
    assert_eq!(std::fs::read(&path).unwrap(), bytes);
}

#[test]
fn writer_coordination_is_held_while_the_source_is_removed() {
    let home = TestRolloutHome::new("compress-held");
    let (path, _) = cold_rollout(&home, 13);
    let probe = RolloutWriterLocks::with_coordination_wait(home.path(), Duration::from_millis(10));
    let hooks = Hooks {
        after_encode: |_: &Path| assert!(probe.lock_coordination().unwrap().is_some(), "no lock while encoding"),
        before_remove: |_: &Path| assert!(probe.lock_coordination().unwrap().is_none(), "writers could start"),
    };
    assert!(matches!(compress_with(&home, &path, &hooks), CompressionOutcome::Compressed { .. }));
}
