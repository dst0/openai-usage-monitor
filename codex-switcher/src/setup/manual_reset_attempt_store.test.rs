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

/// `scripts/uninstall.sh` removes an interrupted staging file only when it
/// matches `^manual-reset-state\.[0-9]+\.[0-9a-f]{16}\.tmp\.json$`; any other
/// name would be left behind as a trace. Small nonces must keep their zero
/// padding, and a write must stage under that name in the home directory.
#[test]
fn staging_name_matches_the_uninstall_pattern() {
    let pid = std::process::id();
    assert_eq!(
        ManualResetAttemptStore::staging_name(1),
        format!("manual-reset-state.{pid}.0000000000000001.tmp.json")
    );
    assert_eq!(
        ManualResetAttemptStore::staging_name(u64::MAX),
        format!("manual-reset-state.{pid}.ffffffffffffffff.tmp.json")
    );
    let env = fixture();
    let files = crate::state_file::fake_state_file_operations::FakeStateFileOperations::new();
    let attempt = ManualResetAttempt::pending(
        "synthetic-account".into(),
        2,
        "00000000-0000-4000-8000-000000000001".into(),
    );
    ManualResetAttemptStore::new(&files)
        .write(&attempt)
        .unwrap();
    let staging = files
        .calls()
        .into_iter()
        .find(|(operation, _)| *operation == "create_staging")
        .expect("a staging file was created")
        .1;
    let home = env.home().to_path_buf();
    drop(env);

    assert_eq!(staging.parent(), Some(home.as_path()));
    let name = staging.file_name().unwrap().to_str().unwrap().to_string();
    let middle = name
        .strip_prefix("manual-reset-state.")
        .and_then(|rest| rest.strip_suffix(".tmp.json"))
        .unwrap_or_else(|| panic!("{name} escapes the uninstall pattern"));
    let (pid, nonce) = middle.split_once('.').expect("pid.nonce");
    assert!(
        !pid.is_empty() && pid.bytes().all(|byte| byte.is_ascii_digit()),
        "{name}"
    );
    assert!(
        nonce.len() == 16
            && nonce
                .bytes()
                .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
        "{name}"
    );
}
