use super::AutomaticDistributionBackoff;
use crate::distribution::distribution_audit_logger::DistributionAuditLogger;
use crate::distribution::distribution_plan::DistributionPlan;
use crate::distribution::distribution_request::DistributionRequest;
use crate::distribution::distribution_transaction_error::DistributionTransactionError;
use crate::distribution::test_helper::TestEnv;
use std::time::{Duration, Instant};

const KEY: &str = "business_priority_return\0app-a\0cli-a\0app-b\0cli-b";
const PHASE: &str = "SHUTDOWN_WINDOW_GUARD_FAILED";
const MINUTE: Duration = Duration::from_secs(60);

fn fail(backoff: &AutomaticDistributionBackoff, now: Instant) -> Option<(u32, Duration)> {
    backoff.record_pre_signal_failure(KEY, PHASE, "WINDOW_ACCESS_FAILED", now)
}

#[test]
fn a_single_failure_is_retried_on_the_next_tick() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();

    assert_eq!(fail(&backoff, now), None);

    assert_eq!(backoff.remaining(KEY, now), None);
}

#[test]
fn the_second_identical_failure_arms_the_base_delay() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    fail(&backoff, now);

    assert_eq!(fail(&backoff, now + MINUTE), Some((2, 5 * MINUTE)));

    assert_eq!(backoff.remaining(KEY, now + MINUTE), Some(5 * MINUTE));
    assert_eq!(backoff.remaining(KEY, now + 3 * MINUTE), Some(3 * MINUTE));
    assert_eq!(backoff.remaining(KEY, now + 6 * MINUTE), None);
}

#[test]
fn later_identical_failures_double_the_delay_up_to_the_cap() {
    let backoff = AutomaticDistributionBackoff::default();
    let mut now = Instant::now();
    let mut delays = Vec::new();
    // Each retry runs when the previous hold ends, as the daemon does.
    for _ in 0..6 {
        let delay = fail(&backoff, now).map(|(_, delay)| delay);
        delays.push(delay);
        now += delay.unwrap_or(MINUTE);
    }

    assert_eq!(
        delays,
        [
            None,
            Some(5 * MINUTE),
            Some(10 * MINUTE),
            Some(20 * MINUTE),
            Some(30 * MINUTE),
            Some(30 * MINUTE),
        ]
    );
}

#[test]
fn a_long_failure_streak_stays_at_the_cap() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    let last = (0..40).map(|_| fail(&backoff, now)).last().flatten();

    assert_eq!(last, Some((40, 30 * MINUTE)));
}

#[test]
fn a_failure_an_hour_after_the_last_one_starts_a_new_streak() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    fail(&backoff, now);
    assert_eq!(fail(&backoff, now + 60 * MINUTE), Some((2, 5 * MINUTE)));

    assert_eq!(fail(&backoff, now + 121 * MINUTE), None);

    assert_eq!(backoff.remaining(KEY, now + 121 * MINUTE), None);
}

#[test]
fn only_the_backed_off_plan_is_held() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    fail(&backoff, now);
    fail(&backoff, now);

    assert_eq!(
        backoff.remaining("quota_exhausted\0app-a\0cli-a\0app-b\0cli-b", now),
        None
    );
    assert_eq!(backoff.remaining(KEY, now), Some(5 * MINUTE));
}

#[test]
fn a_different_failure_or_plan_restarts_the_count() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    fail(&backoff, now);

    let other_message =
        backoff.record_pre_signal_failure(KEY, PHASE, "Desktop process set changed", now);
    let other_phase = backoff.record_pre_signal_failure(
        KEY,
        "WINDOW_CAPTURE_FAILED",
        "Desktop process set changed",
        now,
    );
    let other_plan = backoff.record_pre_signal_failure(
        "other",
        "WINDOW_CAPTURE_FAILED",
        "Desktop process set changed",
        now,
    );

    assert_eq!((other_message, other_phase, other_plan), (None, None, None));
    assert_eq!(backoff.remaining(KEY, now), None);
    assert_eq!(fail(&backoff, now), None);
    assert_eq!(fail(&backoff, now), Some((2, 5 * MINUTE)));
}

#[test]
fn clearing_forgets_the_failure_streak() {
    let backoff = AutomaticDistributionBackoff::default();
    let now = Instant::now();
    fail(&backoff, now);
    fail(&backoff, now);

    backoff.clear();

    assert_eq!(backoff.remaining(KEY, now), None);
    assert_eq!(fail(&backoff, now), None);
}

#[test]
fn a_poisoned_lock_is_recovered() {
    let backoff = std::sync::Arc::new(AutomaticDistributionBackoff::default());
    let now = Instant::now();
    fail(&backoff, now);
    let holder = backoff.clone();
    let _ = std::thread::spawn(move || {
        let _guard = holder.record.lock().unwrap();
        panic!("poison the backoff lock");
    })
    .join();

    assert_eq!(fail(&backoff, now), Some((2, 5 * MINUTE)));
    backoff.clear();
    assert_eq!(backoff.remaining(KEY, now), None);
}

fn switch_plan() -> DistributionPlan {
    let mut plan = DistributionPlan::no_action(
        Some("app-a".into()),
        Some("cli-a".into()),
        "switch",
        Vec::new(),
    );
    plan.target_app_id = Some("app-b".into());
    plan.target_cli_id = Some("cli-b".into());
    plan
}

fn guard_failure() -> DistributionTransactionError {
    DistributionTransactionError::pre_signal(PHASE, "WINDOW_ACCESS_FAILED".into())
}

fn observe(
    backoff: &AutomaticDistributionBackoff,
    request: &DistributionRequest,
    error: Option<&DistributionTransactionError>,
) {
    let logger = DistributionAuditLogger::default();
    backoff.observe(&logger, "op_test", request, &switch_plan(), error);
}

#[test]
fn the_key_names_the_cause_and_every_account() {
    let request = DistributionRequest::auto("business_priority_return");
    assert_eq!(
        AutomaticDistributionBackoff::key(&request, &switch_plan()),
        KEY
    );
}

#[test]
fn an_automatic_result_that_is_not_a_pre_signal_failure_ends_the_streak() {
    let _env = TestEnv::new("backoff_observe_clears");
    let auto = DistributionRequest::auto("business_priority_return");
    for other in [
        None,
        Some(DistributionTransactionError::from("after the signal")),
    ] {
        let backoff = AutomaticDistributionBackoff::default();
        observe(&backoff, &auto, Some(&guard_failure()));

        observe(&backoff, &auto, other.as_ref());

        observe(&backoff, &auto, Some(&guard_failure()));
        assert_eq!(backoff.remaining(KEY, Instant::now()), None);
    }
}

#[test]
fn manual_requests_neither_count_nor_clear_nor_wait() {
    let env = TestEnv::new("backoff_observe_manual");
    let auto = DistributionRequest::auto("business_priority_return");
    let manual = DistributionRequest::user("business_priority_return");
    let backoff = AutomaticDistributionBackoff::default();
    let logger = DistributionAuditLogger::default();

    observe(&backoff, &manual, Some(&guard_failure()));
    observe(&backoff, &manual, Some(&guard_failure()));
    assert_eq!(backoff.remaining(KEY, Instant::now()), None);
    observe(&backoff, &auto, Some(&guard_failure()));
    observe(&backoff, &auto, Some(&guard_failure()));
    observe(&backoff, &manual, None);

    assert!(backoff.remaining(KEY, Instant::now()).is_some());
    assert!(backoff
        .defer(&logger, "op_manual", &manual, &switch_plan())
        .is_none());
    let deferred = backoff
        .defer(&logger, "op_auto", &auto, &switch_plan())
        .expect("the automatic plan is held");
    assert_eq!(
        deferred.status,
        crate::distribution::DistributionStatus::DeferredCooldown
    );
    let log = env.log_content();
    assert_eq!(log.matches("phase=AUTO_BACKOFF_ARMED").count(), 1, "{log}");
    // Only the automatic deferral logs; op IDs are hashed in the audit log.
    assert_eq!(log.matches("phase=AUTO_BACKOFF_ACTIVE").count(), 1, "{log}");
    assert!(!log.contains("trigger=user"), "{log}");
}

#[test]
fn a_hold_is_logged_as_active_once() {
    let env = TestEnv::new("backoff_active_once");
    let auto = DistributionRequest::auto("business_priority_return");
    let backoff = AutomaticDistributionBackoff::default();
    let logger = DistributionAuditLogger::default();
    let now = Instant::now();
    backoff.record_pre_signal_failure(KEY, PHASE, "WINDOW_ACCESS_FAILED", now);
    backoff.record_pre_signal_failure(KEY, PHASE, "WINDOW_ACCESS_FAILED", now);

    for second in 0..3 {
        let at = now + Duration::from_secs(second);
        assert!(backoff
            .defer_at(&logger, "op_held", &auto, &switch_plan(), at)
            .is_some());
    }
    let after_hold = now + 5 * MINUTE;
    assert!(backoff
        .defer_at(&logger, "op_expired", &auto, &switch_plan(), after_hold)
        .is_none());
    backoff.record_pre_signal_failure(KEY, PHASE, "WINDOW_ACCESS_FAILED", after_hold);
    assert!(backoff
        .defer_at(&logger, "op_rearmed", &auto, &switch_plan(), after_hold)
        .is_some());

    let log = env.log_content();
    assert_eq!(log.matches("phase=AUTO_BACKOFF_ACTIVE").count(), 2, "{log}");
    assert!(log.contains("held for 301s"), "{log}");
    assert!(log.contains("held for 601s"), "{log}");
}
