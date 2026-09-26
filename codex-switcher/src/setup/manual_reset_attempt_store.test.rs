use super::{ManualResetAttempt, ManualResetAttemptStore, MAX_JOURNAL_BYTES};
use crate::distribution::test_helper::TestEnv;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> TestEnv {
    TestEnv::new("manual_reset_journal_validation")
}

#[test]
fn journal_load_rejects_broad_file_permissions() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = fixture();
    let attempt = ManualResetAttempt::pending(
        "synthetic-account".into(),
        2,
        "00000000-0000-4000-8000-000000000001".into(),
    );
    ManualResetAttemptStore::write(&attempt).unwrap();
    std::fs::set_permissions(
        ManualResetAttemptStore::path(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let rejected = ManualResetAttemptStore::load().is_err();
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(rejected, "broad journal permissions were accepted");
}

#[test]
fn journal_load_rejects_malformed_unknown_and_oversized_payloads() {
    let guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let env = fixture();
    let path = ManualResetAttemptStore::path();
    std::fs::write(&path, b"{malformed").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let malformed_rejected = ManualResetAttemptStore::load().is_err();

    let attempt = ManualResetAttempt::pending(
        "synthetic-account".into(),
        2,
        "00000000-0000-4000-8000-000000000001".into(),
    );
    let mut unknown = serde_json::to_value(&attempt).unwrap();
    unknown["unexpected_field"] = serde_json::Value::Bool(true);
    std::fs::write(&path, serde_json::to_vec(&unknown).unwrap()).unwrap();
    let unknown_rejected = ManualResetAttemptStore::load().is_err();

    std::fs::write(&path, vec![b'x'; MAX_JOURNAL_BYTES as usize + 1]).unwrap();
    let oversized_rejected = ManualResetAttemptStore::load().is_err();
    drop(env);
    std::env::remove_var("CODEX_HOME");
    drop(guard);
    assert!(malformed_rejected && unknown_rejected && oversized_rejected);
}
