use super::SessionCompressionScheduler;
use std::cell::Cell;
use std::io;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn quick_child() -> io::Result<Child> {
    Command::new("/usr/bin/true").stdin(Stdio::null()).spawn()
}

fn wait_until_reaped<S, E>(scheduler: &mut SessionCompressionScheduler<S, E>, now: Instant)
where
    S: FnMut() -> io::Result<Child>,
    E: Fn() -> bool,
{
    for _ in 0..200 {
        scheduler.poll(now);
        if !scheduler.is_running() {
            return;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("child was never reaped");
}

type Scheduler<'a> = SessionCompressionScheduler<Box<dyn FnMut() -> io::Result<Child> + 'a>, Box<dyn Fn() -> bool + 'a>>;

fn scheduler<'a>(starts: &'a Cell<u32>, enabled: &'a Cell<bool>, start: Instant) -> Scheduler<'a> {
    SessionCompressionScheduler::new(
        Box::new(move || {
            starts.set(starts.get() + 1);
            quick_child()
        }),
        Box::new(move || enabled.get()),
        start,
    )
}

#[test]
fn waits_for_the_first_delay_then_runs_once_a_day() {
    let (starts, enabled) = (Cell::new(0), Cell::new(true));
    let t0 = Instant::now();
    let first = Scheduler::FIRST_DELAY;
    let mut scheduler = scheduler(&starts, &enabled, t0);

    assert!(!scheduler.poll(t0));
    assert!(!scheduler.poll(t0 + first - Duration::from_secs(1)));
    assert!(scheduler.poll(t0 + first));
    assert_eq!(starts.get(), 1);
    wait_until_reaped(&mut scheduler, t0 + first);
    assert!(!scheduler.poll(t0 + first + Duration::from_secs(3600)));
    assert!(scheduler.poll(t0 + first + Scheduler::INTERVAL));
    assert_eq!(starts.get(), 2);
}

#[test]
fn never_starts_a_second_pass_while_one_runs() {
    let starts = Cell::new(0);
    let t0 = Instant::now();
    let mut scheduler = SessionCompressionScheduler::new(
        || {
            starts.set(starts.get() + 1);
            Command::new("/bin/sleep").arg("2").stdin(Stdio::null()).spawn()
        },
        || true,
        t0,
    );
    let late = t0 + Scheduler::FIRST_DELAY + Scheduler::INTERVAL * 3;
    assert!(scheduler.poll(t0 + Scheduler::FIRST_DELAY));
    assert!(!scheduler.poll(late), "the running pass must be awaited");
    assert_eq!(starts.get(), 1);
    wait_until_reaped(&mut scheduler, late);
}

#[test]
fn disabled_compression_rechecks_hourly_without_starting() {
    let (starts, enabled) = (Cell::new(0), Cell::new(false));
    let t0 = Instant::now();
    let due = t0 + Scheduler::FIRST_DELAY;
    let mut scheduler = scheduler(&starts, &enabled, t0);
    assert!(!scheduler.poll(due));
    enabled.set(true);
    assert!(!scheduler.poll(due + Scheduler::RETRY - Duration::from_secs(1)));
    assert!(scheduler.poll(due + Scheduler::RETRY));
    assert_eq!(starts.get(), 1);
    wait_until_reaped(&mut scheduler, due + Scheduler::RETRY);
}

#[test]
fn a_failed_start_is_retried_after_an_hour() {
    let attempts = Cell::new(0);
    let t0 = Instant::now();
    let due = t0 + Scheduler::FIRST_DELAY;
    let mut scheduler = SessionCompressionScheduler::new(
        || {
            attempts.set(attempts.get() + 1);
            Err(io::Error::other("no executable"))
        },
        || true,
        t0,
    );
    assert!(!scheduler.poll(due));
    assert!(!scheduler.poll(due + Duration::from_secs(60)));
    assert_eq!(attempts.get(), 1);
    assert!(!scheduler.poll(due + Scheduler::RETRY));
    assert_eq!(attempts.get(), 2);
}
