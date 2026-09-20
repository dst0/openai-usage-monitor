use super::*;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::{AtomicU64, Ordering};

static TEST_SEQUENCE: AtomicU64 = AtomicU64::new(1);

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
        id: "monitor-account".into(),
        name: None,
        email: "user@example.invalid".into(),
        plan_type: "team".into(),
        account_id: "account-id".into(),
        tokens: crate::models::AuthTokens {
            access_token: "not-a-real-token".into(),
            refresh_token: None,
            id_token: None,
            account_id: Some("account-id".into()),
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
