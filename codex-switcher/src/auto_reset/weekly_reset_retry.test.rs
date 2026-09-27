use super::fixture::{
    edit_account, mismatch_registry_route, prior_attempt, rotate_live_auth, run, settings,
    snapshot, switch_active_account, Home, BLOCKED_TASK, OTHER_TASK, PRIOR_EVENT, PRIOR_KEY,
};
use super::WeeklyResetService;
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::unresolved_auto_reset_for;
use crate::models::AccountConfig;
use crate::quota::ResetCreditConsumeOutcome;

type EnvironmentFactory = fn() -> FakeWeeklyResetEnvironment;
type SnapshotChange = fn(&mut AccountConfig);

const UNRESOLVED: [&str; 2] = ["pending", "unknown"];

fn blocked() -> FakeWeeklyResetEnvironment {
    FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK])
}

/// A persisted attempt may already have reached the service. A refusal on a
/// retry must neither send nor rewrite it, and rotation stays suppressed. The
/// status reports why this retry was refused, not the journal's old reason.
#[test]
fn refused_retry_keeps_a_possibly_sent_attempt_unresolved() {
    let refusals: [(&str, EnvironmentFactory, &str); 8] = [
        (
            "live auth rotated",
            || blocked().during_detection(rotate_live_auth),
            "retry_refused:active_auth_changed_before_reset",
        ),
        (
            "active account switched",
            || blocked().during_detection(switch_active_account),
            "retry_refused:active_account_or_auto_reset_policy_changed",
        ),
        (
            "weekly pool restored",
            || {
                blocked()
                    .during_detection(|| edit_account(|a| a.last_weekly_percentage = Some(35.0)))
            },
            "retry_refused:active_account_or_weekly_quota_changed",
        ),
        (
            "weekly window marker changed",
            || {
                blocked().during_detection(|| {
                    edit_account(|a| a.last_weekly_reset_time = Some("2026-01-15T00:00:00Z".into()))
                })
            },
            "retry_refused:weekly_reset_window_changed",
        ),
        (
            "credit spent",
            || blocked().during_detection(|| edit_account(|a| a.last_credits = Some(0))),
            "retry_refused:no_credit",
        ),
        (
            "reset window closing",
            || {
                blocked().during_detection(|| {
                    edit_account(|a| a.last_weekly_reset_after_seconds = Some(3_600))
                })
            },
            "retry_refused:waiting_for_window",
        ),
        (
            "registry route mismatch",
            || blocked().during_detection(mismatch_registry_route),
            "retry_refused:active_account_route_mismatch",
        ),
        (
            "desktop closed",
            || blocked().with_desktop(Ok(false)),
            "retry_refused:desktop_not_running",
        ),
    ];
    for state in UNRESOLVED {
        for (label, environment, reason) in refusals {
            let home = Home::prepare();
            let before = home.write_journal(&prior_attempt(state));
            let environment = environment();
            let report = run(&environment);
            let after = home.journal_bytes();
            let route_blocked = unresolved_auto_reset_for(&snapshot());
            drop(home);

            let report = report.unwrap_or_else(|error| panic!("{state}/{label}: {error}"));
            assert!(
                environment.requests().is_empty(),
                "{state}/{label}: a request was sent"
            );
            assert_eq!(
                Some(before),
                after,
                "{state}/{label}: the attempt was rewritten"
            );
            assert!(
                report.status.state == state && report.suppress_auto_switch,
                "{state}/{label}: rotation was allowed during an uncertain attempt"
            );
            assert_eq!(
                report.status.reason.as_deref(),
                Some(reason),
                "{state}/{label}: stale refusal reason"
            );
            assert_eq!(
                report.status.last_event_at.as_deref(),
                Some(PRIOR_EVENT),
                "{state}/{label}: the attempt's own event time was replaced"
            );
            assert_eq!(
                route_blocked,
                Ok(true),
                "{state}/{label}: manual reset unblocked"
            );
        }
    }
}

/// The daemon snapshot alone can refuse a retry before any lock or task scan.
#[test]
fn snapshot_refusal_reports_why_the_retry_waits() {
    let cases: [(&str, SnapshotChange, &str); 4] = [
        (
            "weekly pool available",
            |a| a.last_weekly_percentage = Some(35.0),
            "retry_refused:weekly_pool_available",
        ),
        (
            "usage read failed",
            |a| a.last_error = Some("synthetic".into()),
            "retry_refused:active_account_usage_read_failed",
        ),
        (
            "no credit",
            |a| a.last_credits = Some(0),
            "retry_refused:no_credit",
        ),
        (
            "window closing",
            |a| a.last_weekly_reset_after_seconds = Some(3_600),
            "retry_refused:waiting_for_window",
        ),
    ];
    for state in UNRESOLVED {
        for (label, change, reason) in cases {
            let home = Home::prepare();
            let before = home.write_journal(&prior_attempt(state));
            let mut active = snapshot();
            change(&mut active);
            let environment = blocked();
            let report =
                WeeklyResetService::maybe_consume_weekly_reset(&settings(), &active, &environment)
                    .unwrap_or_else(|error| panic!("{state}/{label}: {error}"));
            let after = home.journal_bytes();
            drop(home);

            assert_eq!(environment.detections(), 0, "{state}/{label}");
            assert!(environment.requests().is_empty(), "{state}/{label}");
            assert_eq!(Some(before), after, "{state}/{label}: rewritten");
            assert_eq!(
                (
                    report.status.state.as_str(),
                    report.status.reason.as_deref(),
                    report.suppress_auto_switch
                ),
                (state, Some(reason), true),
                "{state}/{label}"
            );
        }
    }
}

/// `Unavailable` means only that this retry never left the host. The first
/// request with the same key may have been applied, so nothing is demoted.
#[test]
fn unavailable_retry_keeps_a_possibly_sent_attempt_unresolved() {
    for state in UNRESOLVED {
        let home = Home::prepare();
        let before = home.write_journal(&prior_attempt(state));
        let environment = blocked().with_outcome(ResetCreditConsumeOutcome::Unavailable(
            "active_account_route_mismatch".into(),
        ));
        let report = run(&environment).unwrap();
        let after = home.journal_bytes();
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        drop(home);

        assert_eq!(
            environment.requests().len(),
            1,
            "{state}: retry was not attempted"
        );
        assert_eq!(Some(before), after, "{state}: the attempt was demoted");
        assert!(
            report.status.state == state && report.suppress_auto_switch,
            "{state}"
        );
        assert_eq!(
            report.status.reason.as_deref(),
            Some("retry_unavailable:active_account_route_mismatch"),
            "{state}: stale refusal reason"
        );
        assert_eq!(route_blocked, Ok(true), "{state}: manual reset unblocked");
    }
}

#[test]
fn desktop_probe_failure_on_retry_leaves_the_attempt_untouched() {
    for state in UNRESOLVED {
        let home = Home::prepare();
        let before = home.write_journal(&prior_attempt(state));
        let environment = blocked().with_desktop(Err("synthetic probe failure".into()));
        let result = run(&environment);
        let after = home.journal_bytes();
        drop(home);

        assert!(
            result.is_err(),
            "{state}: probe failure was treated as an answer"
        );
        assert!(
            environment.requests().is_empty(),
            "{state}: a request was sent"
        );
        assert_eq!(Some(before), after, "{state}: the attempt was rewritten");
    }
}

/// An eligible retry of an attempt already marked unresolved is sent with the
/// persisted key while the marker stays in place.
#[test]
fn eligible_retry_reuses_the_persisted_key_without_demoting_it_first() {
    for state in UNRESOLVED {
        let home = Home::prepare();
        home.write_journal(&prior_attempt(state));
        let environment = blocked().with_outcome(ResetCreditConsumeOutcome::NotConsumed(
            "nothing_to_reset".into(),
        ));
        let report = run(&environment).unwrap();
        let journal = crate::auto_reset::load_journal_at(&home.journal_path()).unwrap();
        drop(home);

        assert_eq!(
            environment.requests(),
            vec![(
                state.to_string(),
                Some(PRIOR_KEY.to_string()),
                PRIOR_KEY.to_string()
            )],
            "{state}"
        );
        assert!(
            journal.state == "not_consumed" && !report.suppress_auto_switch,
            "{state}"
        );
    }
}

/// Earlier refusals that never sent a request are re-marked `pending` before
/// the retry leaves, so a crash mid-request stays unresolved.
#[test]
fn retry_from_an_unsent_state_is_pending_when_sent() {
    for state in ["waiting_for_desktop", "waiting_for_service"] {
        let home = Home::prepare();
        home.write_journal(&prior_attempt(state));
        let environment = blocked().with_outcome(ResetCreditConsumeOutcome::Unknown(
            "synthetic_transport".into(),
        ));
        let report = run(&environment).unwrap();
        let journal = crate::auto_reset::load_journal_at(&home.journal_path()).unwrap();
        drop(home);

        assert_eq!(
            environment.requests(),
            vec![(
                "pending".to_string(),
                Some(PRIOR_KEY.to_string()),
                PRIOR_KEY.to_string()
            )],
            "{state}"
        );
        assert!(
            journal.state == "unknown" && report.suppress_auto_switch,
            "{state}"
        );
    }
}

/// Only the task that justified the original attempt may drive its retry.
#[test]
fn retry_waits_for_the_original_task_without_sending() {
    let cases: [(&str, &[&str], &str, bool); 5] = [
        ("pending", &[OTHER_TASK], "waiting_for_original_task", true),
        ("pending", &[], "waiting_for_original_task", true),
        ("unknown", &[], "waiting_for_original_task", true),
        (
            "waiting_for_desktop",
            &[OTHER_TASK],
            "waiting_for_original_task",
            false,
        ),
        ("waiting_for_desktop", &[], "waiting_for_task", false),
    ];
    for (state, tasks, expected_state, suppress) in cases {
        let home = Home::prepare();
        let before = home.write_journal(&prior_attempt(state));
        let environment = FakeWeeklyResetEnvironment::new(tasks);
        let report = run(&environment).unwrap();
        let after = home.journal_bytes();
        drop(home);

        assert!(environment.requests().is_empty(), "{state}/{tasks:?}: sent");
        assert_eq!(Some(before), after, "{state}/{tasks:?}: journal rewritten");
        assert_eq!(
            (
                report.status.state.as_str(),
                report.status.reason.as_deref(),
                report.suppress_auto_switch
            ),
            (
                expected_state,
                Some("original_reset_task_is_no_longer_quota_blocked"),
                suppress
            ),
            "{state}/{tasks:?}"
        );
    }
}

#[test]
fn persisted_attempt_without_an_anchor_stops_as_journal_error() {
    let home = Home::prepare();
    let mut journal = prior_attempt("waiting_for_desktop");
    journal.thread_id = None;
    let before = home.write_journal(&journal);
    let environment = blocked();
    let report = run(&environment).unwrap();
    let after = home.journal_bytes();
    drop(home);

    assert!(environment.requests().is_empty());
    assert_eq!(Some(before), after);
    assert_eq!(
        (
            report.status.state.as_str(),
            report.status.reason.as_deref(),
            report.suppress_auto_switch
        ),
        (
            "journal_error",
            Some("auto_reset_journal_missing_anchor"),
            true
        )
    );
}
