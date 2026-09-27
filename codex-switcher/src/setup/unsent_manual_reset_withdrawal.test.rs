use super::manual_reset_attempt::ManualResetAttempt;
use super::manual_reset_attempt_store::ManualResetAttemptStore;
use super::reset_account_transaction_with;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::quota::ResetCreditConsumeOutcome;
use crate::state_file::fake_state_file_operations::FakeStateFileOperations;
use crate::state_file::SystemStateFileOperations;
use std::cell::Cell;

/// `open_for_read` calls in one transaction: the unresolved-attempt check,
/// the readback after the pending write, then the withdrawal's own read.
const READBACK: usize = 2;
const WITHDRAWAL_READ: usize = 3;
/// `sync_directory` and `replace` calls: the pending write, then withdrawal.
const PENDING_WRITE: usize = 1;
const WITHDRAWAL_WRITE: usize = 2;
const OTHER_KEY: &str = "00000000-0000-4000-8000-00000000000f";

fn setup() -> TestEnv {
    let env = TestEnv::new("manual_reset_withdrawal");
    env.populate(
        vec![TestAccountSpec {
            id: "main",
            email: "owner@example.test",
            plan: "team",
            weekly_pct: Some(0.0),
            credits: 2,
            ..TestAccountSpec::default()
        }
        .build()],
        Some("main"),
        None,
    );
    env
}

/// Runs one manual reset with scripted attempt-store faults and returns the
/// result and the number of reset requests that left.
fn reset_with(files: &FakeStateFileOperations) -> (Result<(String, bool), String>, usize) {
    let calls = Cell::new(0);
    let result = reset_account_transaction_with(
        &ManualResetAttemptStore::new(files),
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    (result, calls.get())
}

fn recorded_state() -> Option<String> {
    let bytes = std::fs::read(ManualResetAttemptStore::path()).ok()?;
    let value: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    value["state"].as_str().map(str::to_string)
}

/// Proves the next explicit reset is no longer blocked and does send.
fn next_reset_sends() -> bool {
    reset_with(&FakeStateFileOperations::new()) == (Ok(("main".into(), true)), 1)
}

#[test]
fn unflushed_attempt_is_withdrawn_before_any_request() {
    let env = setup();
    let files = FakeStateFileOperations::new().fail("sync_directory", PENDING_WRITE);
    let (result, sent) = reset_with(&files);
    let state = recorded_state();
    let blocks_automatic = crate::setup::unresolved_manual_reset();
    let unblocked = next_reset_sends();
    drop(env);

    assert_eq!(sent, 0, "a request left without a durable attempt");
    assert_eq!(
        state.as_deref(),
        Some("resolved"),
        "unsent attempt stranded"
    );
    assert_eq!(
        blocks_automatic,
        Ok(false),
        "automatic reset stayed blocked"
    );
    assert!(unblocked, "a later explicit reset stayed blocked");
    let error = result.expect_err("an unflushed attempt must not be sent");
    assert!(
        error.contains("no reset request was sent") && error.contains("was withdrawn"),
        "{error}"
    );
    assert!(files.unfired().is_empty());
}

#[test]
fn transient_readback_failure_withdraws_the_unsent_attempt() {
    let env = setup();
    let files = FakeStateFileOperations::new().fail("open_for_read", READBACK);
    let (result, sent) = reset_with(&files);
    let state = recorded_state();
    let blocks_automatic = crate::setup::unresolved_manual_reset();
    drop(env);

    assert_eq!(sent, 0);
    assert_eq!(
        state.as_deref(),
        Some("resolved"),
        "unsent attempt stranded"
    );
    assert_eq!(blocks_automatic, Ok(false));
    let error = result.expect_err("an unconfirmed attempt must not be sent");
    assert!(error.contains("was withdrawn"), "{error}");
    assert!(files.unfired().is_empty());
}

/// Without a readable record, ownership cannot be proven: fail closed.
#[test]
fn unreadable_attempt_is_left_for_reconciliation() {
    let env = setup();
    let files = FakeStateFileOperations::new()
        .fail("open_for_read", READBACK)
        .fail("open_for_read", WITHDRAWAL_READ);
    let (result, sent) = reset_with(&files);
    let state = recorded_state();
    let blocks_automatic = crate::setup::unresolved_manual_reset();
    drop(env);

    let error = result.expect_err("an unconfirmed attempt must not be sent");
    assert_eq!(sent, 0);
    assert!(
        error.contains("no reset request was sent") && error.contains("could not be read"),
        "{error}"
    );
    assert_eq!(state.as_deref(), Some("pending"));
    assert_eq!(
        blocks_automatic,
        Ok(true),
        "an unproven attempt was released"
    );
    assert!(files.unfired().is_empty());
}

/// A record that is not this command's may be another operation's attempt.
#[test]
fn different_attempt_found_on_readback_is_left_unchanged() {
    let env = setup();
    let other = ManualResetAttempt::pending("main".into(), 2, OTHER_KEY.into());
    let files = FakeStateFileOperations::new().before("open_for_read", READBACK, move || {
        ManualResetAttemptStore::new(&SystemStateFileOperations)
            .write(&other)
            .unwrap()
    });
    let (result, sent) = reset_with(&files);
    let bytes = std::fs::read(ManualResetAttemptStore::path()).unwrap();
    drop(env);

    let error = result.expect_err("a mismatched readback must not be sent");
    assert_eq!(sent, 0);
    assert!(error.contains("left unchanged"), "{error}");
    let saved: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(saved["idempotency_key"], OTHER_KEY);
    assert_eq!(saved["state"], "pending");
    assert!(files.unfired().is_empty());
}

#[test]
fn failed_withdrawal_leaves_the_attempt_unresolved() {
    let env = setup();
    let files = FakeStateFileOperations::new()
        .fail("sync_directory", PENDING_WRITE)
        .fail("replace", WITHDRAWAL_WRITE);
    let (result, sent) = reset_with(&files);
    let state = recorded_state();
    let blocks_automatic = crate::setup::unresolved_manual_reset();
    drop(env);

    let error = result.expect_err("an unflushed attempt must not be sent");
    assert_eq!(sent, 0);
    assert!(error.contains("could not be durably withdrawn"), "{error}");
    assert_eq!(state.as_deref(), Some("pending"));
    assert_eq!(blocks_automatic, Ok(true));
    assert!(files.unfired().is_empty());
}

/// A write that failed before its rename left the earlier resolved record in
/// place; there is nothing of this command's to withdraw.
#[test]
fn attempt_that_never_replaced_the_record_changes_nothing() {
    let env = setup();
    assert!(next_reset_sends(), "setup reset failed");
    let before = std::fs::read(ManualResetAttemptStore::path()).unwrap();
    let files = FakeStateFileOperations::new().fail("replace", PENDING_WRITE);
    let (result, sent) = reset_with(&files);
    let after = std::fs::read(ManualResetAttemptStore::path()).unwrap();
    let blocks_automatic = crate::setup::unresolved_manual_reset();
    drop(env);

    let error = result.expect_err("an unsaved attempt must not be sent");
    assert_eq!(sent, 0);
    assert!(error.contains("no reset request was sent"), "{error}");
    assert_eq!(before, after, "the previous resolved record was rewritten");
    assert_eq!(blocks_automatic, Ok(false));
    assert!(files.unfired().is_empty());
}
