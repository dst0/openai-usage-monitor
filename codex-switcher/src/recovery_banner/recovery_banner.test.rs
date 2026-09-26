use super::{
    BannerSessionStatus, ProcessIdentity, RecoveryBannerOwner, RecoveryBannerService,
    RecoverySession, SavedWindow, WindowRect, BANNER_EXPLANATION,
    BANNER_EXPLANATION_WITHOUT_RESTORE, BANNER_TITLE,
};
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

fn temp_home(label: &str) -> PathBuf {
    let stamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let path = std::env::temp_dir().join(format!("codex-recovery-banner-{label}-{stamp}"));
    fs::create_dir_all(&path).expect("temporary home");
    path
}

fn geometry() -> SavedWindow {
    SavedWindow::new(
        WindowRect::new(-1910.0, -395.0, 1400.0, 900.0).unwrap(),
        WindowRect::new(-2560.0, -500.0, 2560.0, 1440.0).unwrap(),
    )
    .unwrap()
}

#[test]
fn payload_is_private_atomic_and_contains_all_sanitized_rows() {
    let home = temp_home("payload");
    let first = RecoverySession::from_raw(
        "/Users/dst/dev/openai-usage-monitor/",
        "Investigate restart loop\nwith details",
        "123e4567-e89b-12d3-a456-426614174000",
        BannerSessionStatus::Pending,
    );
    let second = RecoverySession::from_raw(
        "Project B",
        "Restore state",
        "thread-987654321",
        BannerSessionStatus::InProgress,
    );
    let process = ProcessIdentity::new(4242, "12345:000006").unwrap();
    let service =
        RecoveryBannerService::begin(&home, "op-test-1", process, geometry(), vec![first, second])
            .expect("banner service starts");
    let payload_path = service.payload_path().to_path_buf();
    let bytes = fs::read(&payload_path).expect("payload exists");
    let mode = fs::metadata(&payload_path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    let payload: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(payload["title"], BANNER_TITLE);
    assert_eq!(payload["explanation"], BANNER_EXPLANATION);
    assert_eq!(payload["expected_process"]["pid"], 4242);
    assert_eq!(payload["saved_window"]["frame"]["x"], -1910.0);
    assert_eq!(payload["sessions"].as_array().unwrap().len(), 2);
    let serialized = String::from_utf8(bytes).unwrap();
    assert!(!serialized.contains("/Users/dst/dev/openai-usage-monitor"));
    assert!(!serialized.contains("123e4567-e89b-12d3-a456-426614174000"));
    assert!(serialized.contains("openai-usage-monitor"));
    assert!(serialized.contains("Investigate restart loop with details"));
    drop(service);
    assert!(!payload_path.exists());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn row_status_updates_are_dynamic_and_unknown_targets_fail_closed() {
    let home = temp_home("status");
    let row = RecoverySession::from_raw(
        "Project",
        "Task",
        "thread-one",
        BannerSessionStatus::Pending,
    );
    let service = RecoveryBannerService::begin(
        &home,
        "op-test-2",
        ProcessIdentity::new(4243, "12346:000006").unwrap(),
        geometry(),
        vec![row],
    )
    .unwrap();
    service
        .update_status("threadone", BannerSessionStatus::InProgress)
        .unwrap();
    service.skip_window_restore().unwrap();
    let payload = service.read_payload().unwrap();
    assert_eq!(payload.explanation, BANNER_EXPLANATION_WITHOUT_RESTORE);
    assert_eq!(payload.saved_window, geometry());
    assert_eq!(
        service.read_payload().unwrap().sessions[0].status,
        BannerSessionStatus::InProgress
    );
    assert!(service
        .update_status("missing", BannerSessionStatus::Failed)
        .is_err());
    service.finish().unwrap();
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn display_owner_is_single_process_and_releases_after_drop() {
    let home = temp_home("owner");
    let path = home.join("recovery-runs/display.lock");
    let first = RecoveryBannerOwner::acquire(&path).unwrap();
    assert!(RecoveryBannerOwner::acquire(&path).is_err());
    drop(first);
    let second = RecoveryBannerOwner::acquire(&path).unwrap();
    assert_eq!(second.path(), path.as_path());
    fs::remove_dir_all(home).unwrap();
}

#[test]
fn rows_preserve_all_targets_for_accessible_overflow() {
    let rows: Vec<_> = (0..40)
        .map(|index| {
            RecoverySession::from_raw(
                format!("Project {index}"),
                format!("Task {index}"),
                format!("thread-{index}"),
                BannerSessionStatus::Pending,
            )
        })
        .collect();
    assert_eq!(rows.len(), 40);
    assert_eq!(
        rows.last().unwrap().display_line(),
        "Project 39 / Task 39 · thread39"
    );
}
