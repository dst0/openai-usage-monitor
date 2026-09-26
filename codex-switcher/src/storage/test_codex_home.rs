use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Mutex, MutexGuard};
use std::thread::ThreadId;

static NEXT_HOME: AtomicU64 = AtomicU64::new(0);
/// Serializes every test that points `CODEX_HOME` at a temporary home. It is
/// private so that `TestCodexHome` is the only way to take it: a test that
/// locked it directly around a `TestEnv` would deadlock, because the mutex is
/// not reentrant, and one that unwrapped a poisoned lock would fail every
/// later test instead of the one that panicked.
static TEST_CODEX_HOME_MUTEX: Mutex<()> = Mutex::new(());
/// The home of the guard currently holding `TEST_CODEX_HOME_MUTEX` and the
/// thread that owns it; `codex_home()` accepts nothing else in test builds.
static ACTIVE_HOME: Mutex<Option<(PathBuf, ThreadId)>> = Mutex::new(None);

/// A test's exclusive temporary `CODEX_HOME`.
///
/// `CODEX_HOME` is process-wide, so the guard holds `TEST_CODEX_HOME_MUTEX` for
/// its whole lifetime. Dropping it, including while a failed assertion unwinds,
/// clears `CODEX_HOME` before the mutex is released and removes the directory.
/// A second guard on the same thread panics instead of deadlocking.
pub(crate) struct TestCodexHome {
    path: PathBuf,
    /// `CODEX_HOME` as found after acquiring the lock; tests use it to prove
    /// that the previous holder cleared the variable before releasing it.
    inherited: Option<OsString>,
    _serial: MutexGuard<'static, ()>,
}

impl TestCodexHome {
    pub(crate) fn new(label: &str) -> Self {
        let current = std::thread::current().id();
        assert!(
            active_home().is_none_or(|(_, owner)| owner != current),
            "nested TestCodexHome on one thread would deadlock; reuse the existing guard"
        );
        let serial = serialize(&TEST_CODEX_HOME_MUTEX);
        let path = std::env::temp_dir().join(format!(
            "codex-test-home-{label}-{}-{}",
            std::process::id(),
            NEXT_HOME.fetch_add(1, Ordering::Relaxed)
        ));
        // An earlier process with the same PID may have left this path behind;
        // creating it exclusively refuses to reuse anything that remains.
        let _ = std::fs::remove_dir_all(&path);
        std::fs::create_dir(&path).expect("test CODEX_HOME must be a new directory");
        let path = std::fs::canonicalize(path).expect("test CODEX_HOME must canonicalize");
        let inherited = std::env::var_os("CODEX_HOME");
        std::env::set_var("CODEX_HOME", &path);
        *lock_recovering(&ACTIVE_HOME) = Some((path.clone(), current));
        Self {
            path,
            inherited,
            _serial: serial,
        }
    }

    pub(crate) fn path(&self) -> &Path {
        &self.path
    }
}

pub(crate) fn active_home() -> Option<(PathBuf, ThreadId)> {
    lock_recovering(&ACTIVE_HOME).clone()
}

/// Acquires the serial lock even after a test panicked while holding it. Every
/// holder clears `CODEX_HOME` while unwinding, so the poison flag marks no
/// broken state; clearing it keeps one failed test from failing the others.
fn serialize(mutex: &'static Mutex<()>) -> MutexGuard<'static, ()> {
    mutex.lock().unwrap_or_else(|poisoned| {
        mutex.clear_poison();
        poisoned.into_inner()
    })
}

fn lock_recovering<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

impl Drop for TestCodexHome {
    fn drop(&mut self) {
        *lock_recovering(&ACTIVE_HOME) = None;
        std::env::remove_var("CODEX_HOME");
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

#[cfg(test)]
#[path = "test_codex_home.test.rs"]
mod tests;
