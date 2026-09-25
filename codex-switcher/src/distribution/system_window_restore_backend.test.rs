use super::*;
use serde_json::json;
use std::os::unix::fs::PermissionsExt;

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
        SystemWindowRestoreBackend::helper_failure(b"PROCESS_IDENTITY_REJECTED\n"),
        "PROCESS_IDENTITY_REJECTED"
    );
    assert_eq!(
        SystemWindowRestoreBackend::helper_failure(b"secret and customer data\n"),
        "Codex window restore helper rejected the request"
    );
}

#[test]
fn banner_capture_uses_non_ax_command_and_rejects_changed_process_identity() {
    let directory = std::env::temp_dir().join(format!(
        "codex-banner-capture-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let helper = directory.join("window-helper");
    let expected = ProcessIdentity::new(4242, "1726789012:000007").unwrap();
    for (returned_pid, accepted) in [(4242, true), (4243, false)] {
        let record = json!({
            "process": {"pid": returned_pid, "birth_id": expected.birth_id},
            "frame": {"x": -2560.0, "y": -139.0, "width": 2560.0, "height": 1330.0},
            "screen": {"display_id": 2, "frame": {"x": -2560.0, "y": -339.0, "width": 2560.0, "height": 1440.0}}
        });
        let script = format!(
            "#!/bin/sh\n[ \"$1\" = \"capture-banner-window\" ] && [ \"$3\" = \"4242\" ] && [ \"$5\" = \"1726789012:000007\" ] || exit 2\nprintf '%s' '{}'\n",
            record
        );
        std::fs::write(&helper, script).unwrap();
        std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
        let mut backend = SystemWindowRestoreBackend {
            helper: helper.clone(),
        };
        let result = backend.capture_banner_window(expected.clone());
        assert_eq!(result.is_ok(), accepted);
        if !accepted {
            assert_eq!(result.unwrap_err(), "PROCESS_IDENTITY_REJECTED");
        }
    }
    std::fs::remove_dir_all(directory).unwrap();
}
