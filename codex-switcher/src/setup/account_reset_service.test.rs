use super::manual_reset_attempt_store::ManualResetAttemptStore;
use super::reset_account_transaction_with;
use crate::distribution::test_account_spec::TestAccountSpec;
use crate::distribution::test_helper::TestEnv;
use crate::quota::ResetCreditConsumeOutcome;
use crate::storage::{load_accounts, update_accounts_atomically};
use std::cell::Cell;
use std::os::unix::fs::PermissionsExt;

fn setup() -> TestEnv {
    let env = TestEnv::new("account_reset_commit_race");
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

#[test]
fn pending_auto_reset_for_same_window_blocks_manual_remote_request() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let journal = serde_json::json!({
        "version": 1,
        "episode_key": "main|unknown-reset-window",
        "account_id": "main",
        "thread_id": "synthetic-task",
        "idempotency_key": "00000000-0000-4000-8000-000000000001",
        "state": "pending",
        "reason": null,
        "updated_at": "2026-01-01T00:00:00Z"
    });
    let path = env.home().join("auto-reset-state.json");
    std::fs::write(&path, serde_json::to_vec(&journal).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let calls = Cell::new(0);
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let blocked = result.is_err() && calls.get() == 0;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        blocked,
        "manual reset bypassed the pending auto-reset attempt"
    );
}

#[test]
fn malformed_auto_reset_journal_blocks_manual_remote_request() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let path = env.home().join("auto-reset-state.json");
    std::fs::write(&path, b"malformed synthetic journal").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let calls = Cell::new(0);
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let blocked =
        result.is_err_and(|error| error.contains("Auto-reset journal")) && calls.get() == 0;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(blocked, "malformed auto-reset state permitted manual reset");
}

#[test]
fn incomplete_pending_auto_reset_journal_blocks_manual_remote_request() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let path = env.home().join("auto-reset-state.json");
    std::fs::write(&path, br#"{"version":1,"state":"pending"}"#).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let calls = Cell::new(0);
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let blocked = result.is_err() && calls.get() == 0;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        blocked,
        "incomplete pending auto reset permitted manual spend"
    );
}

#[test]
fn corrected_weekly_marker_does_not_bypass_unknown_auto_reset() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let journal = serde_json::json!({
        "version": 1,
        "episode_key": "main|older-weekly-window",
        "account_id": "main",
        "thread_id": "synthetic-task",
        "idempotency_key": "00000000-0000-4000-8000-000000000001",
        "state": "unknown"
    });
    let path = env.home().join("auto-reset-state.json");
    std::fs::write(&path, serde_json::to_vec(&journal).unwrap()).unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let calls = Cell::new(0);
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let blocked = result.is_err() && calls.get() == 0;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        blocked,
        "a changed weekly marker bypassed an unknown auto reset"
    );
}

#[test]
fn applied_reset_preserves_concurrent_unrelated_registry_changes() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            update_accounts_atomically(|fresh| {
                fresh.settings.auto_switch_enabled = false;
                fresh.accounts[0].name = Some("renamed".into());
                fresh.accounts.insert(
                    0,
                    TestAccountSpec {
                        id: "added",
                        email: "added@example.test",
                        plan: "pro",
                        sprint_pct: 100.0,
                        ..TestAccountSpec::default()
                    }
                    .build(),
                );
                Ok(())
            })
            .unwrap();
            ResetCreditConsumeOutcome::Applied
        },
    );
    let final_registry = load_accounts().unwrap();
    let preserved = result.is_ok()
        && !final_registry.settings.auto_switch_enabled
        && final_registry.accounts.len() == 2
        && final_registry.accounts[0].email == "added@example.test"
        && final_registry.accounts[1].last_credits == Some(1)
        && final_registry.accounts[1].name.as_deref() == Some("renamed");
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        preserved,
        "reset merge failed: result_ok={} off={} count={} moved={} credit={} renamed={}",
        result.is_ok(),
        !final_registry.settings.auto_switch_enabled,
        final_registry.accounts.len(),
        final_registry.accounts[0].email == "added@example.test",
        final_registry
            .accounts
            .get(1)
            .and_then(|account| account.last_credits)
            == Some(1),
        final_registry
            .accounts
            .get(1)
            .and_then(|account| account.name.as_deref())
            == Some("renamed")
    );
}

#[test]
fn applied_reset_token_conflict_keeps_new_token_and_reports_uncertain_cache() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            update_accounts_atomically(|fresh| {
                fresh.accounts[0].tokens.access_token = "rotated-synthetic-token".into();
                Ok(())
            })
            .unwrap();
            ResetCreditConsumeOutcome::Applied
        },
    );
    let final_registry = load_accounts().unwrap();
    let conflict_preserved = result
        .is_err_and(|error| error.contains("consumed") && error.contains("uncertain"))
        && final_registry.accounts[0].tokens.access_token == "rotated-synthetic-token"
        && final_registry.accounts[0].last_credits == Some(2);
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        conflict_preserved,
        "reset conflict overwrote new account state"
    );
}

#[test]
fn applied_reset_credit_conflict_preserves_new_count() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            update_accounts_atomically(|fresh| {
                fresh.accounts[0].last_credits = Some(3);
                Ok(())
            })
            .unwrap();
            ResetCreditConsumeOutcome::Applied
        },
    );
    let final_registry = load_accounts().unwrap();
    let conflict_preserved = result
        .is_err_and(|error| error.contains("consumed") && error.contains("uncertain"))
        && final_registry.accounts[0].last_credits == Some(3);
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        conflict_preserved,
        "reset conflict overwrote a changed credit count"
    );
}

#[test]
fn unknown_manual_reset_blocks_a_new_request_on_the_next_invocation() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let calls = Cell::new(0);
    let first = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Unknown("synthetic_timeout".into())
        },
    );
    let second = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let blocked = first.is_err() && second.is_err() && calls.get() == 1;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(blocked, "an uncertain reset sent a second remote request");
}

#[test]
fn applied_reset_cache_conflict_blocks_a_new_request_on_the_next_invocation() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let calls = Cell::new(0);
    let first = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            update_accounts_atomically(|fresh| {
                fresh.accounts[0].tokens.access_token = "rotated-synthetic-token".into();
                Ok(())
            })
            .unwrap();
            ResetCreditConsumeOutcome::Applied
        },
    );
    let second = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let blocked = first.is_err() && second.is_err() && calls.get() == 1;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        blocked,
        "a consumed reset with cache conflict sent a second request"
    );
}

#[test]
fn pending_attempt_survives_a_crash_before_the_remote_result() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let crashed = std::panic::catch_unwind(|| {
        let _ = reset_account_transaction_with(
            "main",
            |_| Ok(false),
            |_, key| {
                let journal = std::fs::read(ManualResetAttemptStore::path()).unwrap();
                let state: serde_json::Value = serde_json::from_slice(&journal).unwrap();
                assert_eq!(state["state"], "pending");
                assert_eq!(state["before_credits"], 2);
                assert!(state["started_at"].as_str().is_some());
                assert_eq!(state["idempotency_key"].as_str(), Some(key));
                panic!("synthetic crash after pending attempt was persisted")
            },
        );
    });
    let calls = Cell::new(0);
    let next = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let metadata = std::fs::metadata(ManualResetAttemptStore::path()).unwrap();
    let safe = crashed.is_err()
        && next.is_err()
        && calls.get() == 0
        && metadata.permissions().mode() & 0o777 == 0o600;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(safe, "a crash lost the durable private reset attempt");
}

#[test]
fn unsafe_manual_reset_journal_symlink_blocks_remote_request() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let victim = env.home().join("synthetic-victim.txt");
    std::fs::write(&victim, b"keep synthetic data").unwrap();
    std::os::unix::fs::symlink(&victim, ManualResetAttemptStore::path()).unwrap();
    let calls = Cell::new(0);
    let result = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let safe = result.is_err()
        && calls.get() == 0
        && std::fs::read(&victim).unwrap() == b"keep synthetic data";
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(safe, "unsafe journal path permitted a remote reset");
}

#[test]
fn resolved_applied_attempt_allows_a_later_explicit_reset() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = setup();
    let calls = Cell::new(0);
    let first = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let second = reset_account_transaction_with(
        "main",
        |_| Ok(false),
        |_, _| {
            calls.set(calls.get() + 1);
            ResetCreditConsumeOutcome::Applied
        },
    );
    let current = load_accounts().unwrap();
    let allowed = first.is_ok()
        && second.is_ok()
        && calls.get() == 2
        && current.accounts[0].last_credits == Some(0)
        && !ManualResetAttemptStore::load()
            .unwrap()
            .unwrap()
            .is_unresolved();
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(allowed, "a resolved reset blocked a later explicit request");
}
