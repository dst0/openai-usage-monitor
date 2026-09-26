use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

#[test]
fn pending_manual_reset_blocks_auto_reset_before_dispatch_preparation() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("auto_vs_manual_reset");
    env.populate(
        vec![test_account()],
        Some("user@example.invalid:account-id"),
        None,
    );
    let attempt = serde_json::json!({
        "version": 1,
        "target_id": "user@example.invalid:account-id",
        "before_credits": 1,
        "started_at": "2026-01-01T00:00:00Z",
        "idempotency_key": "00000000-0000-4000-8000-000000000001",
        "state": "unknown"
    });
    let path = env.home().join("manual-reset-state.json");
    fs::write(&path, serde_json::to_vec(&attempt).unwrap()).unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let settings = Settings {
        auto_reset_weekly_enabled: true,
        ..Settings::default()
    };
    let result = maybe_consume_weekly_reset(&settings, &test_account());
    let no_auto_journal = !env.home().join("auto-reset-state.json").exists();
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        result.is_ok_and(|report| report.status.state == "waiting_for_manual_reset"
            && report.suppress_auto_switch)
            && no_auto_journal,
        "auto reset ignored an unresolved manual reset"
    );
}

#[test]
fn malformed_manual_reset_journal_blocks_auto_reset() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("auto_vs_malformed_manual");
    env.populate(
        vec![test_account()],
        Some("user@example.invalid:account-id"),
        None,
    );
    let path = env.home().join("manual-reset-state.json");
    fs::write(&path, b"invalid synthetic journal").unwrap();
    fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
    let settings = Settings {
        auto_reset_weekly_enabled: true,
        ..Settings::default()
    };
    let result = maybe_consume_weekly_reset(&settings, &test_account());
    let no_auto_journal = !env.home().join("auto-reset-state.json").exists();
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        result.is_err() && no_auto_journal,
        "malformed manual journal did not fail closed"
    );
}

#[test]
fn any_unresolved_manual_attempt_blocks_auto_across_local_ids() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("other_manual_account");
    env.populate(
        vec![test_account()],
        Some("user@example.invalid:account-id"),
        None,
    );
    let path = env.home().join("manual-reset-state.json");
    let settings = Settings {
        auto_reset_weekly_enabled: true,
        ..Settings::default()
    };
    for target_id in [
        "alias@example.invalid:account-id",
        "other@example.invalid:another-account",
    ] {
        let attempt = serde_json::json!({
            "version": 1,
            "target_id": target_id,
            "before_credits": 1,
            "started_at": "2026-01-01T00:00:00Z",
            "idempotency_key": "00000000-0000-4000-8000-000000000001",
            "state": "unknown"
        });
        fs::write(&path, serde_json::to_vec(&attempt).unwrap()).unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        let report = maybe_consume_weekly_reset(&settings, &test_account()).unwrap();
        assert!(
            report.status.state == "waiting_for_manual_reset" && report.suppress_auto_switch,
            "{target_id} escaped the unresolved manual attempt"
        );
    }
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
}

#[test]
fn corrected_weekly_marker_does_not_overwrite_unknown_auto_attempt() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("auto_changed_marker");
    env.populate(
        vec![test_account()],
        Some("user@example.invalid:account-id"),
        None,
    );
    let journal = ResetJournal {
        episode_key: Some("account-id|older-marker".into()),
        account_id: Some("account-id".into()),
        thread_id: Some("synthetic-task".into()),
        idempotency_key: Some("00000000-0000-4000-8000-000000000001".into()),
        state: "unknown".into(),
        ..ResetJournal::default()
    };
    let path = env.home().join("auto-reset-state.json");
    write_journal_at(&path, &journal).unwrap();
    let settings = Settings {
        auto_reset_weekly_enabled: true,
        ..Settings::default()
    };
    let report = maybe_consume_weekly_reset(&settings, &test_account()).unwrap();
    let display = status_for_active(&settings, Some(&test_account()));
    let mut no_credit = test_account();
    no_credit.last_credits = Some(0);
    let no_credit_report = maybe_consume_weekly_reset(&settings, &no_credit).unwrap();
    let preserved = load_journal_at(&path).unwrap().episode_key == journal.episode_key;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        preserved
            && report.status.state == "waiting_for_previous_reset"
            && report.suppress_auto_switch
            && display.state == "waiting_for_previous_reset"
            && no_credit_report.suppress_auto_switch,
        "a changed weekly marker hid the uncertain prior auto attempt"
    );
}

#[test]
fn unresolved_auto_attempt_for_another_account_keeps_single_slot_journal() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("auto_other_route");
    env.populate(
        vec![test_account()],
        Some("user@example.invalid:account-id"),
        None,
    );
    let journal = ResetJournal {
        episode_key: Some("other-route|older-marker".into()),
        account_id: Some("other-route".into()),
        thread_id: Some("synthetic-task".into()),
        idempotency_key: Some("00000000-0000-4000-8000-000000000001".into()),
        state: "unknown".into(),
        ..ResetJournal::default()
    };
    let path = env.home().join("auto-reset-state.json");
    write_journal_at(&path, &journal).unwrap();
    let settings = Settings {
        auto_reset_weekly_enabled: true,
        ..Settings::default()
    };
    let report = maybe_consume_weekly_reset(&settings, &test_account()).unwrap();
    let preserved = load_journal_at(&path).unwrap().episode_key == journal.episode_key;
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(
        preserved
            && report.status.state == "waiting_for_previous_reset"
            && report.suppress_auto_switch,
        "another account's request could replace the sole unresolved auto journal"
    );
}

#[test]
fn restored_quota_does_not_erase_an_uncertain_auto_reset_attempt() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = crate::distribution::test_helper::TestEnv::new("auto_uncertain_cleanup");
    let mut account = test_account();
    account.last_weekly_percentage = Some(50.0);
    env.populate(
        vec![account.clone()],
        Some("user@example.invalid:account-id"),
        None,
    );
    let settings = Settings {
        auto_reset_weekly_enabled: true,
        ..Settings::default()
    };
    let path = env.home().join("auto-reset-state.json");
    for state in ["pending", "unknown"] {
        let journal = ResetJournal {
            episode_key: Some(weekly_reset_policy::episode_key(&account)),
            account_id: Some(account.account_id.clone()),
            thread_id: Some("synthetic-task".into()),
            idempotency_key: Some("00000000-0000-4000-8000-000000000001".into()),
            state: state.into(),
            ..ResetJournal::default()
        };
        write_journal_at(&path, &journal).unwrap();
        weekly_reset_status_service::WeeklyResetStatusService::clear_completed_episode_if_restored(
            &settings,
            Some(&account),
        )
        .unwrap();
        assert_eq!(
            load_journal_at(&path).unwrap().state,
            state,
            "restored quota erased a possible in-flight reset"
        );
        let report = maybe_consume_weekly_reset(&settings, &account).unwrap();
        assert!(
            report.suppress_auto_switch && report.status.state == state,
            "restored quota allowed rotation during an unresolved reset"
        );
        assert_eq!(
            status_for_active(&settings, Some(&account)).state,
            state,
            "status hid the unresolved reset after quota restoration"
        );
    }
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
}

#[test]
fn journal_is_private_and_atomically_readable() {
    let root = std::env::temp_dir().join(format!(
        "codex-auto-reset-test-{}-{}",
        std::process::id(),
        TEST_SEQUENCE.fetch_add(1, Ordering::Relaxed)
    ));
    let path = root.join("auto-reset-state.json");
    let journal = ResetJournal {
        episode_key: Some("account|window".into()),
        account_id: Some("account".into()),
        thread_id: Some("00000000-0000-0000-0000-000000000000".into()),
        idempotency_key: Some("opaque-key".into()),
        state: "pending".into(),
        updated_at: Some("2026-01-01T00:00:00Z".into()),
        ..ResetJournal::default()
    };
    write_journal_at(&path, &journal).unwrap();
    let metadata = fs::metadata(&path).unwrap();
    assert_eq!(metadata.permissions().mode() & 0o777, 0o600);
    assert_eq!(load_journal_at(&path).unwrap().state, "pending");
    fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();
    assert!(load_journal_at(&path).is_err());
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn threshold_is_strict_when_nonzero() {
    let mut account = test_account();
    account.last_weekly_reset_after_seconds = Some(86_400);
    assert!(!threshold_eligible(&account, 86_400).unwrap());
    account.last_weekly_reset_after_seconds = Some(86_401);
    assert!(threshold_eligible(&account, 86_400).unwrap());
    assert!(threshold_eligible(&account, 0).unwrap());
}

#[test]
fn exact_zero_is_required_for_weekly_exhaustion() {
    let mut account = test_account();
    account.last_weekly_percentage = Some(0.01);
    assert!(!weekly_exhausted(&account));
    account.last_weekly_percentage = Some(0.0);
    assert!(weekly_exhausted(&account));
}

#[test]
fn old_unsupported_state_is_retryable_via_the_service() {
    assert!(!terminal_no_spend_state("unsupported"));
    assert!(terminal_no_spend_state("not_consumed"));
}

#[test]
fn idempotency_keys_are_uuid_v4_values() {
    let key = new_idempotency_key().unwrap();
    assert_eq!(key.len(), 36);
    assert_eq!(&key[14..15], "4");
    assert!(matches!(key.as_bytes()[19], b'8' | b'9' | b'a' | b'b'));
}

fn test_account() -> AccountConfig {
    AccountConfig {
        id: "user@example.invalid:account-id".into(),
        name: None,
        email: "user@example.invalid".into(),
        plan_type: "team".into(),
        account_id: "account-id".into(),
        tokens: crate::models::AuthTokens {
            access_token: "not-a-real-token".into(),
            refresh_token: None,
            id_token: None,
            account_id: Some("account-id".into()),
            extra: Default::default(),
        },
        enabled: true,
        priority: 0,
        last_primary_percentage: 0.0,
        last_reset_time: None,
        last_reset_after_seconds: None,
        last_weekly_percentage: Some(0.0),
        last_weekly_reset_time: Some("2026-01-08T00:00:00Z".into()),
        last_weekly_reset_after_seconds: Some(100_000),
        last_credits: Some(1),
        last_error: None,
        last_checked: None,
        plan_multiplier: None,
        multiplier_is_manual: None,
        last_multiplier_checked: None,
        organization_name: None,
    }
}
