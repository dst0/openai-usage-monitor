use std::fs::{File, FileTimes};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

static NEXT_HOME: AtomicU64 = AtomicU64::new(0);

/// A private Codex home for compression tests. Code under test receives the
/// path explicitly, so these tests never set or read `CODEX_HOME`.
pub(crate) struct TestRolloutHome {
    path: PathBuf,
}

impl TestRolloutHome {
    pub(crate) const DAY: Duration = Duration::from_secs(24 * 60 * 60);

    pub(crate) fn new(label: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "codex-rollout-compression-{label}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).expect("test home must be new");
        let path = std::fs::canonicalize(path).expect("test home must canonicalize");
        Self { path }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }

    /// A deterministic thread ID for test number `n`.
    pub(crate) fn thread_id(n: u32) -> String {
        format!("019d4e9f-6555-79f1-84b6-{n:012x}")
    }

    /// Rollout lines whose first record is the `session_meta` Codex writes.
    pub(crate) fn rollout_bytes(thread_id: &str, records: usize) -> Vec<u8> {
        let mut bytes = format!(
            "{{\"timestamp\":\"2026-01-02T03:04:05.000Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{thread_id}\",\"cwd\":\"/tmp\"}}}}\n"
        )
        .into_bytes();
        for n in 0..records {
            bytes.extend_from_slice(
                format!("{{\"type\":\"response_item\",\"payload\":{{\"type\":\"message\",\"n\":{n},\"text\":\"repeated output block {}\"}}}}\n", n % 7)
                    .as_bytes(),
            );
        }
        bytes
    }

    /// Writes `bytes` as the rollout of `thread_id` under `root` and dates it
    /// `age` in the past.
    pub(crate) fn write_rollout(
        &self,
        root: &str,
        thread_id: &str,
        bytes: &[u8],
        age: Duration,
    ) -> PathBuf {
        let directory = if root == "sessions" {
            self.path.join(root).join("2026/01/02")
        } else {
            self.path.join(root)
        };
        std::fs::create_dir_all(&directory).unwrap();
        let path = directory.join(format!("rollout-2026-01-02T03-04-05-{thread_id}.jsonl"));
        std::fs::write(&path, bytes).unwrap();
        Self::set_age(&path, age);
        path
    }

    pub(crate) fn set_age(path: &Path, age: Duration) {
        let modified = SystemTime::now() - age;
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_times(FileTimes::new().set_modified(modified))
            .unwrap();
    }

    pub(crate) fn writer_lock_path(&self, thread_id: &str) -> PathBuf {
        self.path
            .join("thread-writer-locks")
            .join(format!("{thread_id}.lock"))
    }
}

impl Drop for TestRolloutHome {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}
