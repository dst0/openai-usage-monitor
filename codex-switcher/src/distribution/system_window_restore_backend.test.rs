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

#[test]
fn running_desktop_recovery_uses_windowserver_and_continues_without_visible_window() {
    let directory = std::env::temp_dir().join(format!(
        "codex-running-banner-test-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir(&directory).unwrap();
    let helper = directory.join("window-helper");
    let script = "#!/bin/sh\ncase \"$1\" in\n  inspect-process) printf '%s' '{\"pid\":4242,\"birth_id\":\"1726789012:000007\"}' ;;\n  capture-banner-window) [ \"$3\" = 4242 ] && [ \"$5\" = '1726789012:000007' ] || exit 2; printf 'WINDOW_NOT_FOUND\\n' >&2; exit 1 ;;\n  *) exit 3 ;;\nesac\n";
    std::fs::write(&helper, script).unwrap();
    std::fs::set_permissions(&helper, std::fs::Permissions::from_mode(0o700)).unwrap();
    let mut backend = SystemWindowRestoreBackend {
        helper: helper.clone(),
    };
    let banner = crate::recovery::RecoveryBanner::start_with_backend(
        "test-running-desktop",
        &["00000000-0000-0000-0000-000000000001".to_string()],
        "thread_recovery",
        4242,
        &mut backend,
    )
    .expect("an absent window must not stop owner-routed task recovery");
    assert_eq!(banner.expected_process().pid, 4242);
    assert_eq!(banner.expected_process().birth_id, "1726789012:000007");
    drop(banner);
    let invalid_capture = "#!/bin/sh\ncase \"$1\" in\n  inspect-process) printf '%s' '{\"pid\":4242,\"birth_id\":\"1726789012:000007\"}' ;;\n  capture-banner-window) printf '%s' 'invalid data' ;;\n  *) exit 3 ;;\nesac\n";
    std::fs::write(&helper, invalid_capture).unwrap();
    assert!(crate::recovery::RecoveryBanner::start_with_backend(
        "test-running-desktop",
        &["00000000-0000-0000-0000-000000000001".to_string()],
        "thread_recovery",
        4242,
        &mut backend,
    )
    .is_err());
    let marker = directory.join("inspected-once");
    let changed_process = format!(
        "#!/bin/sh\ncase \"$1\" in\n  inspect-process) if [ -f '{}' ]; then printf '%s' '{{\"pid\":4242,\"birth_id\":\"1726789012:000008\"}}'; else : > '{}'; printf '%s' '{{\"pid\":4242,\"birth_id\":\"1726789012:000007\"}}'; fi ;;\n  capture-banner-window) printf 'WINDOW_NOT_FOUND\\n' >&2; exit 1 ;;\n  *) exit 3 ;;\nesac\n",
        marker.display(),
        marker.display()
    );
    std::fs::write(&helper, changed_process).unwrap();
    let result = crate::recovery::RecoveryBanner::start_with_backend(
        "test-running-desktop",
        &["00000000-0000-0000-0000-000000000001".to_string()],
        "thread_recovery",
        4242,
        &mut backend,
    );
    assert!(result.is_err(), "a recycled PID must not permit recovery");
    for failure in ["WINDOW_ACCESS_FAILED", "WINDOW_GEOMETRY_FAILED"] {
        let script = format!(
            "#!/bin/sh\ncase \"$1\" in\n  inspect-process) printf '%s' '{{\"pid\":4242,\"birth_id\":\"1726789012:000007\"}}' ;;\n  capture-banner-window) printf '{}\\n' >&2; exit 1 ;;\n  *) exit 3 ;;\nesac\n",
            failure
        );
        std::fs::write(&helper, script).unwrap();
        assert!(crate::recovery::RecoveryBanner::start_with_backend(
            "test-running-desktop",
            &["00000000-0000-0000-0000-000000000001".to_string()],
            "thread_recovery",
            4242,
            &mut backend,
        )
        .is_err());
    }
    let valid_capture = json!({
        "process": {"pid": 4242, "birth_id": "1726789012:000007"},
        "frame": {"x": 0.0, "y": 0.0, "width": 1200.0, "height": 800.0},
        "screen": {"display_id": 1, "frame": {"x": 0.0, "y": 0.0, "width": 1600.0, "height": 900.0}}
    });
    let visible_window = format!(
        "#!/bin/sh\ncase \"$1\" in\n  inspect-process) printf '%s' '{{\"pid\":4242,\"birth_id\":\"1726789012:000007\"}}' ;;\n  capture-banner-window) printf '%s' '{}' ;;\n  *) exit 3 ;;\nesac\n",
        valid_capture
    );
    std::fs::write(&helper, visible_window).unwrap();
    let mut panel_attempted = false;
    let result = crate::recovery::RecoveryBanner::start_with_backend_and_panel(
        "test-running-desktop",
        &["00000000-0000-0000-0000-000000000001".to_string()],
        "thread_recovery",
        4242,
        &mut backend,
        |_, _, _, placement| {
            panel_attempted = true;
            assert_eq!(placement.process.pid, 4242);
            Err("Recovery banner helper did not confirm a visible panel".into())
        },
    );
    assert!(panel_attempted);
    assert_eq!(
        result.err().as_deref(),
        Some("Recovery banner helper did not confirm a visible panel")
    );
    std::fs::remove_dir_all(directory).unwrap();
}
