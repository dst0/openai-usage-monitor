use super::fixture::{
    edit_account, run, snapshot, write_live_tokens, Home, BLOCKED_TASK, OTHER_TASK,
};
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::{load_journal_at, unresolved_auto_reset_for};
use crate::models::WhamUsageResponse;
use crate::quota::ResetCreditConsumeOutcome;
use crate::state_file::fake_state_file_operations::FakeStateFileOperations;

const REFRESH_UNVERIFIED: &str = "reset_applied_quota_refresh_unverified";
const REFRESH_FAILED: &str = "reset_applied_quota_refresh_failed";
const RECOVERY_UNVERIFIED: &str = "reset_applied_recovery_unverified:synthetic_owner_missing";

fn usage(account_id: Option<&str>, weekly_used_percent: Option<f64>) -> WhamUsageResponse {
    let rate_limit = weekly_used_percent
        .map(|used| serde_json::json!({ "secondary_window": { "used_percent": used } }));
    serde_json::from_value(serde_json::json!({
        "account_id": account_id,
        "rate_limit": rate_limit,
    }))
    .unwrap()
}

fn restored() -> Result<WhamUsageResponse, String> {
    Ok(usage(Some("account-id"), Some(0.0)))
}

fn applied(tasks: &[&str]) -> FakeWeeklyResetEnvironment {
    FakeWeeklyResetEnvironment::new(tasks).with_outcome(ResetCreditConsumeOutcome::Applied)
}

/// The confirmed reset is durable as `applied` before the fresh usage read and
/// the recovery hand-off, recovery receives every blocked task, rotation stays
/// suppressed, and a later tick neither spends again nor re-recovers.
#[test]
fn applied_reset_is_durable_before_the_follow_up_and_is_never_repeated() {
    let home = Home::prepare();
    let environment = applied(&[BLOCKED_TASK, OTHER_TASK])
        .with_usage(restored())
        .during_detection(|| {
            // A same-account token rotation: the follow-up must use the copy
            // that was actually sent, not the daemon's earlier snapshot.
            edit_account(|a| a.tokens.access_token = "rotated-not-a-real-token".into());
            write_live_tokens(|t| t.access_token = "rotated-not-a-real-token".into());
        });
    let report = run(&environment).unwrap();
    let journal = load_journal_at(&home.journal_path()).unwrap();
    let settled_bytes = home.journal_bytes();
    let route_blocked = unresolved_auto_reset_for(&snapshot());
    let later = applied(&[BLOCKED_TASK]).with_usage(restored());
    let later_report = run(&later).unwrap();
    let later_bytes = home.journal_bytes();
    drop(home);

    let key = journal
        .idempotency_key
        .clone()
        .expect("attempt keeps its key");
    assert_eq!(
        environment.requests(),
        vec![("pending".to_string(), Some(key.clone()), key)]
    );
    let reads = environment.usage_reads();
    assert_eq!(reads.len(), 1, "exactly one fresh usage read");
    assert_eq!(
        reads[0].0, "applied",
        "usage read before `applied` was durable"
    );
    assert_eq!(reads[0].1.tokens.access_token, "rotated-not-a-real-token");
    assert_eq!(
        environment.recoveries(),
        vec![(
            "applied".to_string(),
            vec![BLOCKED_TASK.to_string(), OTHER_TASK.to_string()]
        )],
        "recovery hand-off"
    );
    assert_eq!(
        (journal.state.as_str(), journal.reason.as_deref()),
        ("applied", None)
    );
    assert_eq!(
        (
            report.status.state.as_str(),
            report.status.reason.as_deref()
        ),
        ("applied", None)
    );
    assert_eq!(report.status.last_event_at, journal.updated_at);
    assert!(report.suppress_auto_switch, "rotation during a fresh reset");
    assert_eq!(
        route_blocked,
        Ok(false),
        "a settled reset blocked manual reset"
    );

    assert!(
        later.requests().is_empty(),
        "the applied episode spent again"
    );
    assert!(later.recoveries().is_empty() && later.usage_reads().is_empty());
    assert_eq!(
        later_bytes, settled_bytes,
        "the applied journal was rewritten"
    );
    assert!(later_report.status.state == "applied" && later_report.suppress_auto_switch);
}

/// An unverified follow-up is recorded next to the credit that was spent; it
/// never demotes `applied`, skips recovery, or releases rotation.
#[test]
fn unverified_follow_up_is_recorded_without_demoting_the_reset() {
    let owner_missing = || Err("synthetic_owner_missing".to_string());
    let combined = format!("{REFRESH_FAILED};{RECOVERY_UNVERIFIED}");
    let cases: [(&str, FakeWeeklyResetEnvironment, &str); 6] = [
        (
            "usage for another account",
            applied(&[BLOCKED_TASK]).with_usage(Ok(usage(Some("other-route"), Some(0.0)))),
            REFRESH_UNVERIFIED,
        ),
        (
            "weekly pool still exhausted",
            applied(&[BLOCKED_TASK]).with_usage(Ok(usage(Some("account-id"), Some(100.0)))),
            REFRESH_UNVERIFIED,
        ),
        (
            "no weekly window",
            applied(&[BLOCKED_TASK]).with_usage(Ok(usage(Some("account-id"), None))),
            REFRESH_UNVERIFIED,
        ),
        (
            "usage read failed",
            applied(&[BLOCKED_TASK]).with_usage(Err("synthetic_network".into())),
            REFRESH_FAILED,
        ),
        (
            "recovery unverified",
            applied(&[BLOCKED_TASK])
                .with_usage(restored())
                .with_recovery(owner_missing()),
            RECOVERY_UNVERIFIED,
        ),
        (
            "both unverified",
            applied(&[BLOCKED_TASK])
                .with_usage(Err("synthetic_network".into()))
                .with_recovery(owner_missing()),
            &combined,
        ),
    ];
    for (label, environment, expected) in cases {
        let home = Home::prepare();
        let report = run(&environment).unwrap();
        let journal = load_journal_at(&home.journal_path()).unwrap();
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        drop(home);

        assert_eq!(environment.requests().len(), 1, "{label}");
        assert_eq!(
            environment.recoveries(),
            vec![("applied".to_string(), vec![BLOCKED_TASK.to_string()])],
            "{label}: recovery hand-off"
        );
        assert_eq!(
            (journal.state.as_str(), journal.reason.as_deref()),
            ("applied", Some(expected)),
            "{label}: journal"
        );
        assert_eq!(
            (
                report.status.state.as_str(),
                report.status.reason.as_deref(),
                report.suppress_auto_switch
            ),
            ("applied", Some(expected), true),
            "{label}: report"
        );
        assert_eq!(report.status.last_event_at, journal.updated_at, "{label}");
        assert_eq!(route_blocked, Ok(false), "{label}");
    }
}

/// The credit is spent once the service says `Applied`. If the `applied`
/// journal became visible but could not be flushed, the next tick sees
/// `applied` and never retries, so recovery is handed off now and the write is
/// repeated; only a repeated failure is reported.
#[test]
fn visible_but_unflushed_applied_journal_still_hands_off_recovery() {
    for (failed_syncs, reported) in [(&[2][..], false), (&[2, 3][..], true)] {
        let home = Home::prepare();
        let files = failed_syncs
            .iter()
            .fold(FakeStateFileOperations::new(), |files, nth| {
                files.fail("sync_directory", *nth)
            });
        let environment = applied(&[BLOCKED_TASK])
            .with_usage(restored())
            .with_journal_files(files);
        let result = run(&environment);
        let journal = load_journal_at(&home.journal_path()).unwrap();
        drop(home);

        assert_eq!(environment.requests().len(), 1, "{failed_syncs:?}");
        assert_eq!(
            environment.recoveries(),
            vec![("applied".to_string(), vec![BLOCKED_TASK.to_string()])],
            "{failed_syncs:?}: recovery hand-off"
        );
        assert_eq!(journal.state, "applied", "{failed_syncs:?}");
        match result {
            Err(error) if reported => {
                assert!(error.contains("could not be synced"), "{error}")
            }
            Ok(report) if !reported => {
                assert!(report.status.state == "applied" && report.suppress_auto_switch)
            }
            other => panic!("{failed_syncs:?}: unexpected result {other:?}"),
        }
        assert!(
            environment.journal_fake().unfired().is_empty(),
            "{failed_syncs:?}"
        );
    }
}

/// If `applied` never replaced `pending`, the same-key retry will see the
/// service's idempotent success and recover then; recovering now would leave
/// the task unblocked and the `pending` retry waiting for it forever.
#[test]
fn applied_journal_that_never_landed_defers_recovery_to_the_same_key_retry() {
    let home = Home::prepare();
    let files = FakeStateFileOperations::new().fail("replace", 2);
    let environment = applied(&[BLOCKED_TASK])
        .with_usage(restored())
        .with_journal_files(files);
    let result = run(&environment);
    let journal = load_journal_at(&home.journal_path()).unwrap();
    let route_blocked = unresolved_auto_reset_for(&snapshot());
    let retry = applied(&[BLOCKED_TASK]).with_usage(restored());
    let retried = run(&retry).unwrap();
    drop(home);

    assert!(result.is_err(), "an unsaved outcome reported success");
    assert!(environment.recoveries().is_empty() && environment.usage_reads().is_empty());
    assert_eq!(journal.state, "pending");
    assert_eq!(route_blocked, Ok(true));
    let key = journal.idempotency_key.expect("pending key");
    assert_eq!(
        retry.requests(),
        vec![("pending".to_string(), Some(key.clone()), key)],
        "the retry must reuse the key whose outcome was not saved"
    );
    assert_eq!(retry.recoveries().len(), 1);
    assert_eq!(retried.status.state, "applied");
}
