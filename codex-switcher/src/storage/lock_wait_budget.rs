//! Bounded waits for the registry's advisory locks.
//!
//! The Monitor's `cxi config` writes wait for the switcher lock that every
//! registry writer holds. A holder that never lets go would leave that command,
//! and the Menu Bar rows waiting on it, stuck. A command that must finish runs
//! its work inside `with_lock_wait_budget`: every lock wait on that thread then
//! polls until the budget's deadline and reports `REGISTRY_BUSY` instead of
//! blocking. Without a budget, waits block as before, so the daemon, switches,
//! and recovery keep their ordering.

use std::cell::Cell;
use std::fs::File;
use std::io;
use std::time::{Duration, Instant};

/// The error every budgeted wait reports when its deadline passes. Callers
/// match on it by equality, so it is the exact text.
pub(crate) const REGISTRY_BUSY: &str =
    "Accounts registry is busy: another Codex Monitor operation is holding its lock";

const POLL_INTERVAL: Duration = Duration::from_millis(25);

thread_local! {
    static DEADLINE: Cell<Option<Instant>> = const { Cell::new(None) };
}

/// Runs `work` with every registry lock wait on this thread bounded by
/// `budget`. A nested budget never extends an outer one. The previous deadline
/// comes back when `work` returns, fails, or panics.
pub(crate) fn with_lock_wait_budget<T>(budget: Duration, work: impl FnOnce() -> T) -> T {
    let previous = DEADLINE.with(Cell::get);
    let requested = Instant::now() + budget;
    let deadline = previous.map_or(requested, |outer| outer.min(requested));
    let _restore = RestoreDeadline(previous);
    DEADLINE.with(|current| current.set(Some(deadline)));
    work()
}

struct RestoreDeadline(Option<Instant>);

impl Drop for RestoreDeadline {
    fn drop(&mut self) {
        DEADLINE.with(|current| current.set(self.0));
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LockMode {
    Shared,
    Exclusive,
}

#[derive(Debug)]
pub(crate) enum LockWaitError {
    /// The budget's deadline passed while another holder kept the lock.
    Busy,
    /// Locking failed for another reason.
    Failed(io::Error),
}

impl LockWaitError {
    /// `REGISTRY_BUSY` for a passed deadline, otherwise the caller's message.
    pub(crate) fn describe(self, failed: impl FnOnce(io::Error) -> String) -> String {
        match self {
            Self::Busy => REGISTRY_BUSY.to_string(),
            Self::Failed(error) => failed(error),
        }
    }
}

/// Locks `file` (an `flock`), blocking without a budget, or polling until
/// this thread's budget runs out with one. The last attempt is made at or
/// after the deadline, so a lock released just in time is still taken.
pub(crate) fn lock_file(file: &File, mode: LockMode) -> Result<(), LockWaitError> {
    // fs2's methods by path: std's own `File` lock methods share these names.
    let Some(deadline) = DEADLINE.with(Cell::get) else {
        return match mode {
            LockMode::Shared => fs2::FileExt::lock_shared(file),
            LockMode::Exclusive => fs2::FileExt::lock_exclusive(file),
        }
        .map_err(LockWaitError::Failed);
    };
    loop {
        let attempt = match mode {
            LockMode::Shared => fs2::FileExt::try_lock_shared(file),
            LockMode::Exclusive => fs2::FileExt::try_lock_exclusive(file),
        };
        match attempt {
            Ok(()) => return Ok(()),
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {}
            Err(error) => return Err(LockWaitError::Failed(error)),
        }
        let now = Instant::now();
        if now >= deadline {
            return Err(LockWaitError::Busy);
        }
        std::thread::sleep(POLL_INTERVAL.min(deadline.saturating_duration_since(now)));
    }
}

/// Takes the switcher lock on `file` with the messages its callers report.
pub(super) fn lock_switcher(file: &File, exclusive: bool) -> Result<(), String> {
    let (mode, kind) = if exclusive {
        (LockMode::Exclusive, "exclusive")
    } else {
        (LockMode::Shared, "shared")
    };
    lock_file(file, mode).map_err(|wait| {
        wait.describe(|error| format!("Failed to acquire {kind} switcher lock: {error}"))
    })
}

#[cfg(test)]
pub(crate) fn current_deadline() -> Option<Instant> {
    DEADLINE.with(Cell::get)
}

#[cfg(test)]
#[path = "lock_wait_budget.test.rs"]
mod tests;
