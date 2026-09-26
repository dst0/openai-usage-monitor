use super::*;
use crate::recovery_banner::BannerSessionStatus;

fn visible_banner(
    home: &std::path::Path,
    process: &WindowProcessIdentity,
    id: &str,
) -> RecoveryBanner {
    let window = SavedWindow::new(
        WindowRect::new(0.0, 0.0, 1200.0, 800.0).unwrap(),
        WindowRect::new(0.0, 0.0, 1600.0, 900.0).unwrap(),
    )
    .unwrap();
    let service = RecoveryBannerService::begin(
        home,
        "late-status-test",
        BannerProcessIdentity::new(process.pid, process.birth_id.clone()).unwrap(),
        window,
        vec![RecoverySession::from_raw(
            "project",
            "task",
            id,
            BannerSessionStatus::Pending,
        )],
    )
    .unwrap();
    let child = Command::new("/bin/sleep").arg("30").spawn().unwrap();
    RecoveryBanner {
        expected_process: process.clone(),
        child: Some(child),
        visible_since: Some(Instant::now() - MIN_BANNER_VISIBLE),
        service: Some(service),
        capture: None,
        pending_statuses: Vec::new(),
    }
}

fn test_home(suffix: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!(
        "codex-late-panel-{suffix}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ))
}

#[test]
fn late_panel_replay_updates_its_private_payload() {
    let home = test_home("status");
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut visible = visible_banner(&home, &process, id);
    let mut early = RecoveryBanner::without_window(process);
    early.record_status(id, BannerSessionStatus::InProgress);
    early.record_status(id, BannerSessionStatus::Completed);
    early.replay_pending_statuses_into(&mut visible).unwrap();
    let payload = visible.service.as_ref().unwrap().read_payload().unwrap();
    assert_eq!(payload.sessions[0].status, BannerSessionStatus::Completed);
    drop(visible);
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn failed_late_panel_status_write_keeps_original_statuses() {
    let home = test_home("write-error");
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let id = "01a098c2-0fae-74d2-a80c-45d89e910e79";
    let mut visible = visible_banner(&home, &process, id);
    let mut early = RecoveryBanner::without_window(process);
    early.record_status(id, BannerSessionStatus::InProgress);
    let recovery_dir = home.join("recovery-runs");
    std::fs::remove_dir_all(&recovery_dir).unwrap();
    std::fs::write(&recovery_dir, b"block directory recreation").unwrap();
    assert!(early.replay_pending_statuses_into(&mut visible).is_err());
    assert_eq!(
        early.pending_statuses,
        vec![(id.to_owned(), BannerSessionStatus::InProgress)]
    );
    drop(visible);
    std::fs::remove_file(recovery_dir).unwrap();
    std::fs::remove_dir_all(home).unwrap();
}

#[test]
fn unit_tests_cannot_resolve_the_installed_banner_helper() {
    crate::test_live_system::assert_forbidden(
        "installed recovery-banner helper",
        banner_helper_candidates,
    );
}
