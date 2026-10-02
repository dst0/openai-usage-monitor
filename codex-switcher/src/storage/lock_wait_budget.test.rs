use super::{current_deadline, lock_file, with_lock_wait_budget, LockMode, LockWaitError};
use crate::storage::test_codex_home::TestCodexHome;
use std::fs::{File, OpenOptions};
use std::path::Path;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// A separate open of `path`, so its `flock` conflicts with any other open of
/// the same file, in this process too.
fn open(path: &Path) -> File {
    OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .expect("test lock file must open")
}

/// Holds an exclusive lock on `path` from another thread until `hold` passes,
/// returning once the lock is held.
fn hold_exclusive(path: &Path, hold: Duration) -> thread::JoinHandle<()> {
    let holder = open(path);
    fs2::FileExt::lock_exclusive(&holder).expect("holder must lock");
    thread::spawn(move || {
        thread::sleep(hold);
        drop(holder);
    })
}

#[test]
fn budgeted_wait_reports_busy_once_its_deadline_passes() {
    let home = TestCodexHome::new("lock_budget_busy");
    let path = home.path().join("probe.lock");
    let holder = open(&path);
    fs2::FileExt::lock_exclusive(&holder).unwrap();

    for mode in [LockMode::Shared, LockMode::Exclusive] {
        let started = Instant::now();
        let result =
            with_lock_wait_budget(Duration::from_millis(200), || lock_file(&open(&path), mode));
        let waited = started.elapsed();
        assert!(
            matches!(result, Err(LockWaitError::Busy)),
            "{mode:?} must report busy"
        );
        assert!(
            waited >= Duration::from_millis(200),
            "waited only {waited:?}"
        );
        assert!(waited < Duration::from_secs(5), "waited {waited:?}");
    }
    drop(holder);
}

#[test]
fn budgeted_wait_takes_a_lock_released_before_its_deadline() {
    let home = TestCodexHome::new("lock_budget_released");
    let path = home.path().join("probe.lock");
    let holder = hold_exclusive(&path, Duration::from_millis(150));
    let result = with_lock_wait_budget(Duration::from_secs(10), || {
        lock_file(&open(&path), LockMode::Exclusive)
    });
    assert!(result.is_ok(), "a released lock must be taken: {result:?}");
    holder.join().unwrap();
}

#[test]
fn unbudgeted_wait_still_blocks_until_the_holder_lets_go() {
    let home = TestCodexHome::new("lock_budget_unbounded");
    let path = home.path().join("probe.lock");
    assert_eq!(current_deadline(), None);
    let started = Instant::now();
    let holder = hold_exclusive(&path, Duration::from_millis(300));
    let result = lock_file(&open(&path), LockMode::Exclusive);
    assert!(
        result.is_ok(),
        "an unbudgeted wait must succeed: {result:?}"
    );
    assert!(
        started.elapsed() >= Duration::from_millis(250),
        "it must have waited"
    );
    holder.join().unwrap();
}

#[test]
fn shared_holders_do_not_wait_for_each_other() {
    let home = TestCodexHome::new("lock_budget_shared");
    let path = home.path().join("probe.lock");
    let reader = open(&path);
    fs2::FileExt::lock_shared(&reader).unwrap();
    let result = with_lock_wait_budget(Duration::from_millis(50), || {
        lock_file(&open(&path), LockMode::Shared)
    });
    assert!(result.is_ok(), "a second reader must not wait: {result:?}");
    drop(reader);
}

#[test]
fn budget_is_cleared_after_success_failure_and_panic_and_never_extended() {
    assert_eq!(current_deadline(), None);
    let value = with_lock_wait_budget(Duration::from_secs(1), || 7);
    assert_eq!(value, 7);
    assert_eq!(current_deadline(), None, "success must clear the budget");

    let failed: Result<(), String> =
        with_lock_wait_budget(Duration::from_secs(1), || Err("no".into()));
    assert!(failed.is_err());
    assert_eq!(current_deadline(), None, "failure must clear the budget");

    let panicked = std::panic::catch_unwind(|| {
        with_lock_wait_budget(Duration::from_secs(1), || panic!("work panicked"))
    });
    assert!(panicked.is_err());
    assert_eq!(current_deadline(), None, "a panic must clear the budget");

    with_lock_wait_budget(Duration::from_millis(100), || {
        let outer = current_deadline().expect("outer budget");
        with_lock_wait_budget(Duration::from_secs(60), || {
            assert_eq!(
                current_deadline(),
                Some(outer),
                "a nested budget must not extend"
            );
        });
        with_lock_wait_budget(Duration::from_millis(1), || {
            assert!(
                current_deadline().unwrap() <= outer,
                "a nested budget may shorten"
            );
        });
        assert_eq!(
            current_deadline(),
            Some(outer),
            "the outer budget must come back"
        );
    });
    assert_eq!(current_deadline(), None);
}

#[test]
fn budget_belongs_to_its_own_thread() {
    with_lock_wait_budget(Duration::from_secs(1), || {
        let (sender, receiver) = mpsc::channel();
        thread::spawn(move || sender.send(current_deadline()).unwrap())
            .join()
            .unwrap();
        assert_eq!(receiver.recv().unwrap(), None);
    });
}
