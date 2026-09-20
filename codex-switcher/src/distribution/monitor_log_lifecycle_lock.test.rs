use super::monitor_log_lifecycle_lock::MonitorLogLifecycleLock;
use super::LogPermissionsService;
use std::fs;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

fn temporary_home() -> std::path::PathBuf {
    let nonce = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "codex_monitor_lifecycle_lock_{}_{}_{}",
        std::process::id(),
        nonce,
        sequence
    ));
    fs::create_dir_all(&path).unwrap();
    fs::canonicalize(path).unwrap()
}

#[test]
fn exclusive_migration_lock_blocks_shared_writer_until_release() {
    let home = temporary_home();
    MonitorLogLifecycleLock::ensure(&home).unwrap();
    let exclusive = MonitorLogLifecycleLock::exclusive(&home).unwrap();
    let (started_tx, started_rx) = mpsc::channel();
    let (acquired_tx, acquired_rx) = mpsc::channel();
    let worker_home = home.clone();
    let worker = thread::spawn(move || {
        started_tx.send(()).unwrap();
        let _shared = MonitorLogLifecycleLock::shared(&worker_home).unwrap();
        acquired_tx.send(()).unwrap();
    });

    started_rx.recv().unwrap();
    assert!(acquired_rx.recv_timeout(Duration::from_millis(50)).is_err());
    drop(exclusive);
    acquired_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    worker.join().unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn shared_writer_cannot_recreate_an_unlinked_lifecycle_inode() {
    let home = temporary_home();
    MonitorLogLifecycleLock::ensure(&home).unwrap();
    let _exclusive = MonitorLogLifecycleLock::exclusive(&home).unwrap();
    let lock_path = home.join("log/.monitor-log-lifecycle.lock");
    fs::remove_file(&lock_path).unwrap();

    let error = match MonitorLogLifecycleLock::shared(&home) {
        Ok(_) => panic!("writer recreated an unlinked lifecycle lock"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!lock_path.exists());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn preopened_shared_waiter_rejects_an_inode_unlinked_while_waiting() {
    let home = temporary_home();
    MonitorLogLifecycleLock::ensure(&home).unwrap();
    let exclusive = MonitorLogLifecycleLock::exclusive(&home).unwrap();
    let (opened_tx, opened_rx) = mpsc::channel();
    let worker_home = home.clone();
    let worker = thread::spawn(move || {
        MonitorLogLifecycleLock::acquire_with_before_lock(&worker_home, false, || {
            opened_tx.send(()).unwrap();
        })
    });

    opened_rx.recv_timeout(Duration::from_secs(1)).unwrap();
    let log_dir = home.join("log");
    fs::remove_file(log_dir.join(".monitor-log-lifecycle.lock")).unwrap();
    fs::remove_dir(&log_dir).unwrap();
    assert!(LogPermissionsService::enforce_home(&home).is_err());
    assert!(!log_dir.exists());
    drop(exclusive);

    let error = match worker.join().unwrap() {
        Ok(_) => panic!("pre-opened writer acquired an unlinked lifecycle lock"),
        Err(error) => error,
    };
    assert_eq!(error.kind(), std::io::ErrorKind::NotFound);
    assert!(!log_dir.exists());
    fs::remove_dir_all(home).unwrap();
}
