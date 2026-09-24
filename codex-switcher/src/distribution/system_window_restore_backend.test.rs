use super::*;
use serde_json::json;

#[test]
fn preserves_colon_delimited_birth_identity_from_swift_helper() {
    let identity = SystemWindowRestoreBackend::parse_process(&json!({
        "pid": 4242,
        "birth_id": "1726789012:000007"
    }))
    .expect("helper birth identity should parse");

    assert_eq!(identity.pid, 4242);
    assert_eq!(identity.birth_id, "1726789012:000007");
}

#[test]
fn forwards_the_same_opaque_birth_identity_to_helper() {
    let identity = ProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let args = SystemWindowRestoreBackend::args_for_process("read-window", identity);

    assert_eq!(
        args,
        vec![
            "read-window",
            "--expected-pid",
            "4242",
            "--expected-birth",
            "1726789012:000007"
        ]
    );
}

#[test]
fn rejects_missing_or_control_character_birth_identity() {
    assert!(SystemWindowRestoreBackend::parse_process(&json!({
        "pid": 4242,
        "birth_id": ""
    }))
    .is_err());
    assert!(SystemWindowRestoreBackend::parse_process(&json!({
        "pid": 4242,
        "birth_id": "1726789012:\n"
    }))
    .is_err());
}

#[test]
fn helper_failure_preserves_only_known_window_states() {
    assert_eq!(
        SystemWindowRestoreBackend::helper_failure(b"WINDOW_NOT_FOUND\n"),
        "WINDOW_NOT_FOUND"
    );
    assert_eq!(
        SystemWindowRestoreBackend::helper_failure(b"WINDOW_ACCESS_FAILED\n"),
        "WINDOW_ACCESS_FAILED"
    );
    assert_eq!(
        SystemWindowRestoreBackend::helper_failure(b"secret and customer data\n"),
        "Codex window restore helper rejected the request"
    );
}
