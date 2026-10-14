use std::io;
use std::process::Child;
use std::time::{Duration, Instant};

/// Starts a background compression pass from the daemon about once a day.
///
/// The daemon must stay within its small memory budget, so the pass runs in
/// a separate low-priority process (`codex-mon sessions compress
/// --scheduled`); the scheduler only starts it, reaps it, and never waits.
pub struct SessionCompressionScheduler<S, E>
where
    S: FnMut() -> io::Result<Child>,
    E: Fn() -> bool,
{
    spawn: S,
    enabled: E,
    next_start: Instant,
    child: Option<Child>,
}

impl<S, E> SessionCompressionScheduler<S, E>
where
    S: FnMut() -> io::Result<Child>,
    E: Fn() -> bool,
{
    /// Lets the daemon finish starting up before the first pass.
    pub const FIRST_DELAY: Duration = Duration::from_secs(10 * 60);
    pub const INTERVAL: Duration = Duration::from_secs(24 * 60 * 60);
    /// How soon to look again when compression is off or the start failed.
    pub const RETRY: Duration = Duration::from_secs(60 * 60);

    pub fn new(spawn: S, enabled: E, now: Instant) -> Self {
        Self {
            spawn,
            enabled,
            next_start: now + Self::FIRST_DELAY,
            child: None,
        }
    }

    /// Reaps a finished pass and starts the next one when it is due.
    /// Returns whether a pass was started.
    pub fn poll(&mut self, now: Instant) -> bool {
        if let Some(child) = self.child.as_mut() {
            match child.try_wait() {
                Ok(None) => return false,
                Ok(Some(_)) | Err(_) => self.child = None,
            }
        }
        if now < self.next_start {
            return false;
        }
        if !(self.enabled)() {
            self.next_start = now + Self::RETRY;
            return false;
        }
        match (self.spawn)() {
            Ok(child) => {
                self.child = Some(child);
                self.next_start = now + Self::INTERVAL;
                true
            }
            Err(_) => {
                self.next_start = now + Self::RETRY;
                false
            }
        }
    }

    pub fn is_running(&self) -> bool {
        self.child.is_some()
    }
}

#[cfg(test)]
#[path = "session_compression_scheduler.test.rs"]
mod tests;
