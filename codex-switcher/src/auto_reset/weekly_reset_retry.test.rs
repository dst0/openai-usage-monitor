use super::fixture::{
    edit_account, mismatch_registry_route, prior_attempt, rotate_live_auth, run, snapshot,
    switch_active_account, Home, BLOCKED_TASK, OTHER_TASK, PRIOR_KEY,
};
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::unresolved_auto_reset_for;
use crate::quota::ResetCreditConsumeOutcome;

type EnvironmentFactory = fn() -> FakeWeeklyResetEnvironment;

const UNRESOLVED: [&str; 2] = ["pending", "unknown"];

fn blocked() -> FakeWeeklyResetEnvironment {
    FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK])
}

/// A persisted attempt may already have reached the service. A refusal on a
/// retry must neither send nor rewrite it, and rotation stays suppressed.
#[test]
fn refused_retry_keeps_a_possibly_sent_attempt_unresolved() {
    let refusals: [(&str, EnvironmentFactory); 5] = [
        ("live auth rotated", || {
            blocked().during_detection(rotate_live_auth)
        }),
        ("active account switched", || {
            blocked().during_detection(switch_active_account)
        }),
        ("weekly window marker changed", || {
            blocked().during_detection(|| {
                edit_account(|a| a.last_weekly_reset_time = Some("2026-01-15T00:00:00Z".into()))
            })
        }),
        ("registry route mismatch", || {
            blocked().during_detection(mismatch_registry_route)
        }),
        ("desktop closed", || blocked().with_desktop(Ok(false))),
    ];
    for state in UNRESOLVED {
        for (label, environment) in refusals {
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
                route_blocked,
                Ok(true),
                "{state}/{label}: manual reset unblocked"
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
    let cases: [(&str, &[&str], &str, bool); 4] = [
        ("pending", &[OTHER_TASK], "waiting_for_original_task", true),
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
            (report.status.state.as_str(), report.suppress_auto_switch),
            (expected_state, suppress),
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
