use super::{serialize, TestCodexHome, TEST_CODEX_HOME_MUTEX};
use crate::storage::codex_home;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::sync::{mpsc, Mutex};
use std::time::Duration;

fn panic_message(payload: Box<dyn std::any::Any + Send>) -> String {
    payload
        .downcast_ref::<String>()
        .cloned()
        .or_else(|| payload.downcast_ref::<&str>().map(ToString::to_string))
        .unwrap_or_default()
}

#[test]
fn points_codex_home_at_a_fresh_canonical_directory_and_removes_it() {
    let first = TestCodexHome::new("fresh");
    let path = first.path().to_path_buf();
    assert!(path.is_dir());
    assert_eq!(std::fs::canonicalize(&path).unwrap(), path);
    assert_eq!(std::env::var_os("CODEX_HOME"), Some(path.clone().into()));
    assert_eq!(codex_home(), path);
    std::fs::write(path.join("accounts.json"), b"{}").unwrap();
    drop(first);

    assert!(!path.exists(), "dropping the guard removes its home");
    let second = TestCodexHome::new("fresh");
    assert_ne!(second.path(), path, "every guard gets its own home");
    assert_eq!(
        second.inherited, None,
        "the previous guard cleared CODEX_HOME"
    );
}

#[test]
fn a_second_guard_waits_until_the_first_is_dropped() {
    let first = TestCodexHome::new("exclusive-first");
    let first_path = first.path().to_path_buf();
    let (sender, receiver) = mpsc::channel();
    let waiter = std::thread::spawn(move || {
        let second = TestCodexHome::new("exclusive-second");
        let observed = (second.inherited.clone(), second.path().to_path_buf());
        assert_eq!(codex_home(), observed.1);
        sender.send(observed).unwrap();
    });

    assert!(
        receiver.recv_timeout(Duration::from_millis(200)).is_err(),
        "a second guard must not start while the first holds CODEX_HOME"
    );
    assert_eq!(codex_home(), first_path);
    drop(first);

    let (inherited, second_path) = receiver.recv_timeout(Duration::from_secs(10)).unwrap();
    waiter.join().unwrap();
    assert_eq!(
        inherited, None,
        "CODEX_HOME was cleared before the lock was released"
    );
    assert_ne!(second_path, first_path);
}

#[test]
fn a_panicking_test_clears_codex_home_and_releases_the_lock() {
    let mut observed = None;
    let outcome = catch_unwind(AssertUnwindSafe(|| {
        let home = TestCodexHome::new("panicking");
        observed = Some(home.path().to_path_buf());
        panic!("simulated failed assertion");
    }));
    assert!(outcome.is_err());

    assert!(!observed.unwrap().exists(), "unwinding removes the home");
    // Acquiring again would deadlock or panic if unwinding had kept the
    // lock, and a leaked value would show up as the next guard's inheritance.
    let next = TestCodexHome::new("after-panic");
    assert_eq!(next.inherited, None);
    assert!(!TEST_CODEX_HOME_MUTEX.is_poisoned());
}

#[test]
fn recovers_and_clears_a_poisoned_serial_lock() {
    static SERIAL: Mutex<()> = Mutex::new(());
    let outcome = catch_unwind(|| {
        let _held = SERIAL.lock().unwrap();
        panic!("simulated failed assertion while holding the lock");
    });
    assert!(outcome.is_err());
    assert!(SERIAL.is_poisoned());

    let recovered = serialize(&SERIAL);
    assert!(!SERIAL.is_poisoned());
    drop(recovered);
    assert!(SERIAL.try_lock().is_ok());
}

#[test]
fn a_nested_guard_fails_instead_of_deadlocking() {
    let home = TestCodexHome::new("outer");

    let inner = catch_unwind(|| TestCodexHome::new("inner"));
    let message = panic_message(inner.err().expect("a nested guard must panic"));

    assert!(message.contains("nested TestCodexHome"), "{message}");
    assert_eq!(codex_home(), home.path(), "the outer guard stays in force");
}

#[test]
fn codex_home_refuses_an_unset_home_instead_of_the_live_one() {
    let _home = TestCodexHome::new("unset");
    std::env::remove_var("CODEX_HOME");

    let message = panic_message(catch_unwind(codex_home).unwrap_err());

    assert!(message.contains("CODEX_HOME is unset"), "{message}");
}

#[test]
fn codex_home_refuses_a_value_the_guard_did_not_set() {
    let _home = TestCodexHome::new("replaced");
    let foreign = dirs::home_dir().unwrap().join(".codex");
    std::env::set_var("CODEX_HOME", &foreign);

    let message = panic_message(catch_unwind(codex_home).unwrap_err());

    assert!(message.contains("is not the guard's home"), "{message}");
}

#[test]
fn a_worker_thread_of_the_guarded_test_resolves_its_home() {
    let home = TestCodexHome::new("worker");
    let expected = home.path().to_path_buf();

    let resolved = std::thread::spawn(codex_home).join().unwrap();

    assert_eq!(resolved, expected);
}
