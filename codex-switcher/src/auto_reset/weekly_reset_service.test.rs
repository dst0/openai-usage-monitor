use super::fixture::{
    edit_account, edit_registry, mismatch_registry_route, other_account, rotate_live_auth, run,
    settings, snapshot, switch_active_account, write_live_tokens, Home, BLOCKED_TASK,
};
use super::WeeklyResetService;
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::weekly_reset_policy::unresolved_attempt;
use crate::auto_reset::{load_journal_at, unresolved_auto_reset_for};
use crate::quota::ResetCreditConsumeOutcome;

type Change = fn();
type EnvironmentFactory = fn() -> FakeWeeklyResetEnvironment;

const POLICY: &str = "active_account_or_auto_reset_policy_changed";
const QUOTA: &str = "active_account_or_weekly_quota_changed";
const AUTH: &str = "active_auth_changed_before_reset";

/// Each change lands after the lock-time registry check, while tasks are being
/// detected. No request may be sent and no journal may claim that one was.
/// The expected reason identifies which final check refused.
#[test]
fn change_during_preparation_never_strands_an_unsent_pending_attempt() {
    let scenarios: [(&str, Change, &str, Option<&str>); 13] = [
        (
            "weekly pool restored",
            || edit_account(|a| a.last_weekly_percentage = Some(35.0)),
            "policy_changed",
            Some(QUOTA),
        ),
        (
            "usage read failed",
            || edit_account(|a| a.last_error = Some("synthetic".into())),
            "policy_changed",
            Some(QUOTA),
        ),
        (
            "credit already spent",
            || edit_account(|a| a.last_credits = Some(0)),
            "no_credit",
            None,
        ),
        (
            "reset window closing",
            || edit_account(|a| a.last_weekly_reset_after_seconds = Some(3_600)),
            "waiting_for_window",
            None,
        ),
        (
            "weekly window marker changed",
            || edit_account(|a| a.last_weekly_reset_time = Some("2026-01-15T00:00:00Z".into())),
            "policy_changed",
            Some("weekly_reset_window_changed"),
        ),
        (
            "automatic reset disabled",
            || edit_registry(|r| r.settings.auto_reset_weekly_enabled = false),
            "policy_changed",
            Some(POLICY),
        ),
        (
            "reset threshold changed",
            || edit_registry(|r| r.settings.auto_reset_weekly_min_remaining_seconds = 7_200),
            "policy_changed",
            Some(POLICY),
        ),
        (
            "active account switched",
            switch_active_account,
            "policy_changed",
            Some(POLICY),
        ),
        (
            "active account removed",
            || edit_registry(|r| r.accounts.clear()),
            "policy_changed",
            Some(POLICY),
        ),
        (
            "live access token rotated",
            rotate_live_auth,
            "policy_changed",
            Some(AUTH),
        ),
        (
            "live route changed",
            || write_live_tokens(|t| t.account_id = Some("other-route".into())),
            "policy_changed",
            Some(AUTH),
        ),
        (
            "live auth removed",
            || std::fs::remove_file(crate::storage::auth_json_path()).unwrap(),
            "policy_changed",
            Some(AUTH),
        ),
        (
            "registry route mismatch",
            mismatch_registry_route,
            "policy_changed",
            Some("active_account_route_mismatch"),
        ),
    ];
    for (label, change, expected_state, expected_reason) in scenarios {
        let home = Home::prepare();
        let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).during_detection(change);
        let report = run(&environment);
        let journal = home.journal_bytes();
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        let other = WeeklyResetService::maybe_consume_weekly_reset(
            &settings(),
            &other_account(),
            &FakeWeeklyResetEnvironment::new(&[]),
        );
        drop(home);

        let report = report.unwrap_or_else(|error| panic!("{label}: {error}"));
        assert_eq!(environment.detections(), 1, "{label}: change did not run");
        assert!(
            environment.requests().is_empty(),
            "{label}: a request was sent"
        );
        assert_eq!(journal, None, "{label}: an unsent attempt was persisted");
        assert_eq!(
            (
                report.status.state.as_str(),
                report.status.reason.as_deref(),
                report.suppress_auto_switch
            ),
            (expected_state, expected_reason, false),
            "{label}: wrong refusal"
        );
        assert_eq!(
            route_blocked,
            Ok(false),
            "{label}: manual reset stayed blocked"
        );
        let other = other.unwrap_or_else(|error| panic!("{label}: {error}"));
        assert!(
            other.status.state == "ready" && !other.suppress_auto_switch,
            "{label}: another account stayed behind the unsent attempt ({})",
            other.status.state
        );
    }
}

/// A registry that already disagrees at lock time exits before the (slow)
/// task detection and before any journal is prepared.
#[test]
fn lock_time_account_change_exits_before_task_detection() {
    let home = Home::prepare();
    switch_active_account();
    let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]);
    let report = run(&environment).unwrap();
    let journal = home.journal_bytes();
    drop(home);

    assert_eq!(environment.detections(), 0);
    assert!(environment.requests().is_empty() && journal.is_none());
    assert_eq!(
        (
            report.status.state.as_str(),
            report.status.reason.as_deref(),
            report.suppress_auto_switch
        ),
        (
            "policy_changed",
            Some("active_account_changed_before_reset"),
            false
        )
    );
}

#[test]
fn preflight_errors_send_and_persist_nothing() {
    let failures: [(&str, EnvironmentFactory); 2] = [
        ("desktop probe failed", || {
            FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK])
                .with_desktop(Err("synthetic process inspection failure".into()))
        }),
        ("registry became unreadable", || {
            FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).during_detection(|| {
                std::fs::write(crate::storage::accounts_json_path(), b"not json").unwrap()
            })
        }),
    ];
    for (label, environment) in failures {
        let home = Home::prepare();
        let environment = environment();
        let result = run(&environment);
        let journal = home.journal_bytes();
        drop(home);

        assert!(
            result.is_err(),
            "{label}: an unknown state was treated as an answer"
        );
        assert!(
            environment.requests().is_empty(),
            "{label}: a request was sent"
        );
        assert_eq!(journal, None, "{label}: an unsent attempt was persisted");
    }
}

#[test]
fn closed_desktop_defers_without_pending_and_the_retry_is_pending_when_sent() {
    let home = Home::prepare();
    let closed = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_desktop(Ok(false));
    let deferred = run(&closed).unwrap();
    let waiting = load_journal_at(&home.journal_path()).unwrap();
    let manual_open = unresolved_auto_reset_for(&snapshot());
    let open = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_outcome(
        ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into()),
    );
    let sent = run(&open).unwrap();
    let settled = load_journal_at(&home.journal_path()).unwrap();
    drop(home);

    assert!(
        closed.requests().is_empty(),
        "a request was sent without Desktop"
    );
    assert!(
        deferred.status.state == "waiting_for_desktop" && !deferred.suppress_auto_switch,
        "unexpected deferred report"
    );
    assert!(!unresolved_attempt(&waiting) && waiting.state == "waiting_for_desktop");
    assert_eq!(waiting.thread_id.as_deref(), Some(BLOCKED_TASK));
    assert_eq!(
        manual_open,
        Ok(false),
        "a deferred, unsent attempt blocked manual reset"
    );
    let key = waiting
        .idempotency_key
        .clone()
        .expect("deferred attempt keeps its key");
    assert_eq!(
        open.requests(),
        vec![("pending".to_string(), Some(key.clone()), key)],
        "the retried request left before its pending marker was durable"
    );
    assert!(settled.state == "not_consumed" && !sent.suppress_auto_switch);
}

#[test]
fn ready_attempt_is_pending_on_disk_when_the_request_leaves() {
    let outcomes = [
        (
            ResetCreditConsumeOutcome::Unknown("synthetic_transport".into()),
            "unknown",
            true,
        ),
        (
            ResetCreditConsumeOutcome::Unavailable("synthetic_unavailable".into()),
            "waiting_for_service",
            false,
        ),
        (
            ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into()),
            "not_consumed",
            false,
        ),
    ];
    for (outcome, expected_state, suppress) in outcomes {
        let home = Home::prepare();
        let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_outcome(outcome);
        let report = run(&environment).unwrap();
        let journal = load_journal_at(&home.journal_path()).unwrap();
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        drop(home);

        let key = journal
            .idempotency_key
            .clone()
            .expect("attempt keeps its key");
        assert_eq!(
            environment.requests(),
            vec![("pending".to_string(), Some(key.clone()), key)],
            "{expected_state}: request left without a durable pending marker"
        );
        assert_eq!(journal.state, expected_state);
        assert_eq!(journal.thread_id.as_deref(), Some(BLOCKED_TASK));
        assert_eq!(report.suppress_auto_switch, suppress, "{expected_state}");
        assert_eq!(route_blocked, Ok(suppress), "{expected_state}: manual gate");
    }
}

/// A same-account token rotation during preparation is allowed, and the
/// request must carry the registry's fresh copy rather than the snapshot.
#[test]
fn ready_attempt_sends_the_fresh_registry_copy() {
    let home = Home::prepare();
    let environment = FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK])
        .with_outcome(ResetCreditConsumeOutcome::NotConsumed(
            "nothing_to_reset".into(),
        ))
        .during_detection(|| {
            edit_account(|a| a.tokens.access_token = "rotated-not-a-real-token".into());
            write_live_tokens(|t| t.access_token = "rotated-not-a-real-token".into());
        });
    let report = run(&environment).unwrap();
    drop(home);

    let sent = environment.sent_accounts();
    assert_eq!(sent.len(), 1, "exactly one request");
    assert_eq!(sent[0].tokens.access_token, "rotated-not-a-real-token");
    assert_eq!(sent[0].account_id, snapshot().account_id);
    assert_eq!(report.status.state, "not_consumed");
}
