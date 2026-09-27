use super::fixture::{other_account, prior_attempt, run, settings, snapshot, Home, BLOCKED_TASK};
use crate::auto_reset::fake_weekly_reset_environment::FakeWeeklyResetEnvironment;
use crate::auto_reset::{load_journal_at, unresolved_auto_reset_for};
use crate::models::AccountConfig;
use crate::quota::ResetCreditConsumeOutcome;
use crate::state_file::fake_state_file_operations::FakeStateFileOperations;

/// Journal writes through the fake: the `pending` marker, then withdrawal.
const MARKER_WRITE: usize = 1;
const WITHDRAWAL_WRITE: usize = 2;
const OTHER_KEY: &str = "00000000-0000-4000-8000-00000000000f";

fn blocked_with(files: FakeStateFileOperations) -> FakeWeeklyResetEnvironment {
    FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_journal_files(files)
}

/// Another account that is itself eligible, so only an unresolved journal of
/// this account could make it wait.
fn exhausted_other() -> AccountConfig {
    let mut account = other_account();
    account.last_weekly_percentage = Some(0.0);
    account
}

fn on_disk() -> (String, Option<String>) {
    let journal = load_journal_at(&crate::storage::codex_home().join("auto-reset-state.json"))
        .expect("readable journal");
    (journal.state, journal.reason)
}

fn settled_retry() -> FakeWeeklyResetEnvironment {
    FakeWeeklyResetEnvironment::new(&[BLOCKED_TASK]).with_outcome(
        ResetCreditConsumeOutcome::NotConsumed("nothing_to_reset".into()),
    )
}

/// The marker was renamed into place but its directory could not be flushed.
/// No request left, and the record provably is this attempt's, so it must not
/// stay `pending`: that would suppress rotation and block manual reset and
/// every other account's automatic reset.
#[test]
fn unflushed_pending_marker_is_withdrawn_before_any_request() {
    for prior in [
        None,
        Some("waiting_for_desktop"),
        Some("waiting_for_service"),
    ] {
        let home = Home::prepare();
        if let Some(state) = prior {
            home.write_journal(&prior_attempt(state));
        }
        let environment =
            blocked_with(FakeStateFileOperations::new().fail("sync_directory", MARKER_WRITE));
        let result = run(&environment);
        let withdrawn = load_journal_at(&home.journal_path());
        let route_blocked = unresolved_auto_reset_for(&snapshot());
        // Read-only: the daemon path would also discard the settled journal
        // for a restored account, which would hide the key reuse below.
        let other = crate::auto_reset::status_for_active(&settings(), Some(&exhausted_other()));
        let retry = settled_retry();
        let retried = run(&retry);
        drop(home);

        let label = prior.unwrap_or("new attempt");
        assert!(environment.requests().is_empty(), "{label}: request sent");
        let withdrawn = withdrawn.unwrap();
        assert_eq!(
            (withdrawn.state.as_str(), withdrawn.reason.as_deref()),
            ("journal_error", Some("pending_marker_not_durable")),
            "{label}: unsent marker stranded"
        );
        assert_eq!(
            withdrawn.thread_id.as_deref(),
            Some(BLOCKED_TASK),
            "{label}"
        );
        assert_eq!(route_blocked, Ok(false), "{label}: manual reset blocked");
        assert_eq!(
            other.state, "waiting_for_task",
            "{label}: another account waited behind an unsent attempt"
        );
        let error = result.expect_err("an unflushed marker must not report success");
        assert!(
            error.contains("no reset request was sent") && error.contains("was withdrawn"),
            "{label}: {error}"
        );
        // The withdrawn key never left, so the next eligible tick reuses it
        // behind a fresh, durable `pending` marker.
        let key = withdrawn.idempotency_key.expect("withdrawn key");
        assert_eq!(
            retry.requests(),
            vec![("pending".to_string(), Some(key.clone()), key)],
            "{label}"
        );
        assert_eq!(retried.unwrap().status.state, "not_consumed", "{label}");
        assert!(environment.journal_fake().unfired().is_empty(), "{label}");
    }
}

/// A marker whose rename failed never replaced the journal: nothing to undo.
#[test]
fn marker_that_never_replaced_the_journal_changes_nothing() {
    let home = Home::prepare();
    let environment = blocked_with(FakeStateFileOperations::new().fail("replace", MARKER_WRITE));
    let result = run(&environment);
    let journal = home.journal_bytes();
    let route_blocked = unresolved_auto_reset_for(&snapshot());
    drop(home);

    assert!(environment.requests().is_empty());
    assert_eq!(journal, None, "an unsent attempt was persisted");
    assert_eq!(route_blocked, Ok(false));
    let error = result.expect_err("an unsaved marker must not report success");
    assert!(error.contains("no reset request was sent"), "{error}");
}

/// Without a readable marker, ownership cannot be proven: fail closed and keep
/// it unresolved until a same-key retry or reconciliation settles it.
#[test]
fn unreadable_marker_is_left_unresolved() {
    let home = Home::prepare();
    // The load skips `open_for_read` while no journal exists, so the first
    // open is the withdrawal's own read of the marker; the hook pins that.
    let files = FakeStateFileOperations::new()
        .fail("sync_directory", MARKER_WRITE)
        .before("open_for_read", 1, || {
            let (state, _) = on_disk();
            assert_eq!(state, "pending", "fault hit the wrong read")
        })
        .fail("open_for_read", 1);
    let environment = blocked_with(files);
    let result = run(&environment);
    let journal = load_journal_at(&home.journal_path()).unwrap();
    let route_blocked = unresolved_auto_reset_for(&snapshot());
    drop(home);

    assert!(environment.requests().is_empty());
    assert_eq!(journal.state, "pending");
    assert_eq!(route_blocked, Ok(true), "an unproven marker was released");
    let error = result.expect_err("an unflushed marker must not report success");
    assert!(error.contains("could not be read"), "{error}");
    assert!(environment.journal_fake().unfired().is_empty());
}

/// A journal that is not this attempt's may describe another request.
#[test]
fn different_journal_found_during_withdrawal_is_left_unchanged() {
    let home = Home::prepare();
    let path = home.journal_path();
    let mut other = prior_attempt("unknown");
    other.idempotency_key = Some(OTHER_KEY.into());
    let files = FakeStateFileOperations::new()
        .fail("sync_directory", MARKER_WRITE)
        .before("open_for_read", 1, move || {
            crate::auto_reset::write_journal_at(&path, &other).unwrap()
        });
    let environment = blocked_with(files);
    let result = run(&environment);
    let journal = load_journal_at(&home.journal_path()).unwrap();
    drop(home);

    assert!(environment.requests().is_empty());
    assert_eq!(
        (journal.state.as_str(), journal.idempotency_key.as_deref()),
        ("unknown", Some(OTHER_KEY))
    );
    let error = result.expect_err("an unflushed marker must not report success");
    assert!(error.contains("left unchanged"), "{error}");
    assert!(environment.journal_fake().unfired().is_empty());
}

#[test]
fn failed_withdrawal_leaves_the_marker_unresolved() {
    let home = Home::prepare();
    let files = FakeStateFileOperations::new()
        .fail("sync_directory", MARKER_WRITE)
        .fail("replace", WITHDRAWAL_WRITE);
    let environment = blocked_with(files);
    let result = run(&environment);
    let journal = load_journal_at(&home.journal_path()).unwrap();
    drop(home);

    assert!(environment.requests().is_empty());
    assert_eq!(journal.state, "pending");
    let error = result.expect_err("an unflushed marker must not report success");
    assert!(error.contains("could not be durably withdrawn"), "{error}");
    assert!(environment.journal_fake().unfired().is_empty());
}

/// After the request left, a journal write that fails after its rename is not
/// withdrawn: the visible `unknown` outcome keeps rotation and manual reset
/// blocked until the same-key retry settles it.
#[test]
fn outcome_write_after_the_request_is_never_withdrawn() {
    let home = Home::prepare();
    let files = FakeStateFileOperations::new().fail("sync_directory", 2);
    let environment = blocked_with(files).with_outcome(ResetCreditConsumeOutcome::Unknown(
        "synthetic_transport".into(),
    ));
    let result = run(&environment);
    let journal = load_journal_at(&home.journal_path()).unwrap();
    let route_blocked = unresolved_auto_reset_for(&snapshot());
    drop(home);

    assert_eq!(environment.requests().len(), 1);
    assert!(result.is_err(), "an unflushed outcome reported success");
    assert_eq!(journal.state, "unknown");
    assert_eq!(route_blocked, Ok(true));
    assert!(environment.journal_fake().unfired().is_empty());
}

/// The withdrawal became visible but could not be flushed: nothing is left
/// unresolved on disk, and the message only says it may still show pending.
#[test]
fn unflushed_withdrawal_is_reported() {
    let home = Home::prepare();
    let files = FakeStateFileOperations::new()
        .fail("sync_directory", MARKER_WRITE)
        .fail("sync_directory", WITHDRAWAL_WRITE);
    let environment = blocked_with(files);
    let result = run(&environment);
    let journal = load_journal_at(&home.journal_path()).unwrap();
    let route_blocked = unresolved_auto_reset_for(&snapshot());
    drop(home);

    assert!(environment.requests().is_empty());
    assert_eq!(journal.state, "journal_error");
    assert_eq!(route_blocked, Ok(false));
    let error = result.expect_err("an unflushed marker must not report success");
    assert!(
        error.contains("could not be durably withdrawn and may still"),
        "{error}"
    );
    assert!(environment.journal_fake().unfired().is_empty());
}
