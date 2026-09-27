use super::automation_guard::{
    arm_automation_cooldown_at, automation_cooldown_remaining_at, claim_restart_operation_at,
    cooldown_path, operation_lock, valid_operation_id,
};
use crate::storage::test_codex_home::TestCodexHome;
use std::{
    os::{fd::AsRawFd, unix::fs::PermissionsExt},
    time::{Duration, UNIX_EPOCH},
};

/// Runs `body` while a forked child keeps a copy of every descriptor open in
/// this process, as a child spawned by a concurrent test does until it execs.
/// The child makes only async-signal-safe calls and exits once the parent's
/// pipe end closes, so a panicking `body` cannot leave it blocked.
fn while_a_forked_child_holds_descriptors<T>(body: impl FnOnce() -> T) -> T {
    let (reader, writer) = std::io::pipe().unwrap();
    let (wait_fd, writer_fd) = (reader.as_raw_fd(), writer.as_raw_fd());
    // SAFETY: the child calls only close(2), read(2) and _exit(2), which are
    // async-signal-safe after fork in a multithreaded process, and never
    // returns into the test harness.
    let pid = unsafe { libc::fork() };
    if pid == 0 {
        let mut byte = 0_u8;
        // SAFETY: `byte` outlives the read; see the fork comment above.
        unsafe {
            libc::close(writer_fd);
            while libc::read(wait_fd, (&raw mut byte).cast(), 1) < 0
                && *libc::__error() == libc::EINTR
            {}
            libc::_exit(0);
        }
    }
    assert!(pid > 0, "fork failed");
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(body));
    drop(writer);
    let mut status = 0;
    let reaped = loop {
        // SAFETY: `pid` is this test's own unreaped child.
        let reaped = unsafe { libc::waitpid(pid, &raw mut status, 0) };
        if reaped >= 0 || std::io::Error::last_os_error().raw_os_error() != Some(libc::EINTR) {
            break reaped;
        }
    };
    assert_eq!(reaped, pid);
    result.unwrap_or_else(|panic| std::panic::resume_unwind(panic))
}

/// flock belongs to the open file description, and a child created by fork(2)
/// or posix_spawn(3) copies every descriptor without close-on-fork, O_CLOEXEC
/// ones included, until it execs. A child that a concurrent test spawned kept
/// a released operation lock held, refusing the next switch in the same home.
#[test]
fn a_concurrently_forked_child_cannot_keep_a_released_operation_lock() {
    let _home = TestCodexHome::new("operation-lock-fork");
    let lock = operation_lock().unwrap();
    let relocked = while_a_forked_child_holds_descriptors(|| {
        drop(lock);
        operation_lock().map(drop)
    });
    assert_eq!(relocked, Ok(()));
}

/// `O_CLOFORK` must not weaken exclusion, as per-process fcntl locks would.
#[test]
fn operation_lock_still_refuses_a_second_holder() {
    let _home = TestCodexHome::new("operation-lock-held");
    let _held = operation_lock().unwrap();
    assert_eq!(
        operation_lock().map(drop),
        Err("Another desktop switch/recovery is in progress".to_string())
    );
}

#[test]
fn restart_operation_can_be_claimed_only_once() {
    let root = std::env::temp_dir().join(format!(
        "codex-restart-claim-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    std::fs::create_dir_all(&root).unwrap();
    assert!(claim_restart_operation_at(&root, "12345-678").unwrap());
    assert!(!claim_restart_operation_at(&root, "12345-678").unwrap());
    let metadata = std::fs::metadata(root.join("recovery-runs/restart-12345-678.claimed")).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn restart_operation_rejects_path_components() {
    assert!(!valid_operation_id("../worker"));
    assert!(!valid_operation_id("worker"));
    assert!(valid_operation_id("12345-678"));
}

#[test]
fn durable_automation_cooldown_is_atomic_private_and_expires() {
    let root = std::env::temp_dir().join(format!(
        "codex-cooldown-test-{}-{}",
        std::process::id(),
        std::thread::current().name().unwrap_or("unnamed")
    ));
    std::fs::create_dir_all(&root).unwrap();
    let now = UNIX_EPOCH + Duration::from_secs(1_000_000);
    arm_automation_cooldown_at(&root, now, Duration::from_secs(180)).unwrap();
    assert_eq!(
        automation_cooldown_remaining_at(&root, now).unwrap(),
        Some(Duration::from_secs(180))
    );
    assert_eq!(
        std::fs::metadata(cooldown_path(&root))
            .unwrap()
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
    assert_eq!(
        automation_cooldown_remaining_at(&root, now + Duration::from_secs(180)).unwrap(),
        None
    );
    std::fs::write(cooldown_path(&root), b"invalid\n").unwrap();
    assert!(automation_cooldown_remaining_at(&root, now).is_err());
    std::fs::remove_dir_all(root).unwrap();
}
