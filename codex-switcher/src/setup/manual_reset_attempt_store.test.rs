use super::{ManualResetAttempt, ManualResetAttemptStore, MAX_JOURNAL_BYTES};
use crate::distribution::test_helper::TestEnv;
use crate::state_file::SystemStateFileOperations;
use std::os::unix::fs::PermissionsExt;

fn fixture() -> TestEnv {
    TestEnv::new("manual_reset_journal_validation")
}

#[test]
fn journal_load_rejects_broad_file_permissions() {
    let env = fixture();
    let attempt = ManualResetAttempt::pending(
        "synthetic-account".into(),
        2,
        "00000000-0000-4000-8000-000000000001".into(),
    );
    ManualResetAttemptStore::new(&SystemStateFileOperations)
        .write(&attempt)
        .unwrap();
    std::fs::set_permissions(
        ManualResetAttemptStore::path(),
        std::fs::Permissions::from_mode(0o644),
    )
    .unwrap();
    let rejected = ManualResetAttemptStore::new(&SystemStateFileOperations)
        .load()
        .is_err();
    drop(env);
    assert!(rejected, "broad journal permissions were accepted");
}

#[test]
fn journal_load_rejects_malformed_unknown_and_oversized_payloads() {
    let env = fixture();
    let path = ManualResetAttemptStore::path();
    std::fs::write(&path, b"{malformed").unwrap();
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
    let malformed_rejected = ManualResetAttemptStore::new(&SystemStateFileOperations)
        .load()
        .is_err();

    let attempt = ManualResetAttempt::pending(
        "synthetic-account".into(),
        2,
        "00000000-0000-4000-8000-000000000001".into(),
    );
    let mut unknown = serde_json::to_value(&attempt).unwrap();
    unknown["unexpected_field"] = serde_json::Value::Bool(true);
    std::fs::write(&path, serde_json::to_vec(&unknown).unwrap()).unwrap();
    let unknown_rejected = ManualResetAttemptStore::new(&SystemStateFileOperations)
        .load()
        .is_err();

    std::fs::write(&path, vec![b'x'; MAX_JOURNAL_BYTES as usize + 1]).unwrap();
    let oversized_rejected = ManualResetAttemptStore::new(&SystemStateFileOperations)
        .load()
        .is_err();
    drop(env);
    assert!(malformed_rejected && unknown_rejected && oversized_rejected);
}

/// Each failed stage keeps its own operator-facing message, and none of them
/// includes a path or an I/O detail.
#[test]
fn write_reports_the_stage_that_failed() {
    let cases = [
        (
            "prepare_directory",
            "Manual reset directory could not be prepared",
        ),
        (
            "create_staging",
            "Manual reset staging file could not be created",
        ),
        (
            "save_staging",
            "Manual reset staging file could not be saved",
        ),
        ("replace", "Manual reset attempt could not be replaced"),
        (
            "sync_directory",
            "Manual reset attempt directory sync failed",
        ),
    ];
    for (failing, expected) in cases {
        let env = fixture();
        let files = crate::state_file::fake_state_file_operations::FakeStateFileOperations::new()
            .fail(failing, 1);
        let attempt = ManualResetAttempt::pending(
            "synthetic-account".into(),
            2,
            "00000000-0000-4000-8000-000000000001".into(),
        );
        let result = ManualResetAttemptStore::new(&files).write(&attempt);
        drop(env);
        assert_eq!(result, Err(expected.to_string()), "{failing}");
    }
}
