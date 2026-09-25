use super::*;
use crate::distribution::WindowProcessIdentity;

#[test]
fn status_changes_before_a_late_panel_are_replayed_in_order() {
    let process = WindowProcessIdentity::new(4242, "1726789012:000007").unwrap();
    let mut original = RecoveryBanner::without_window(process.clone());
    original.record_status("task-1", BannerSessionStatus::InProgress);
    original.record_status("task-1", BannerSessionStatus::Completed);
    let mut replacement = RecoveryBanner::without_window(process);
    original
        .replay_pending_statuses_into(&mut replacement)
        .unwrap();
    assert!(original.pending_statuses.is_empty());
    assert_eq!(
        replacement.pending_statuses,
        vec![
            ("task-1".to_owned(), BannerSessionStatus::InProgress),
            ("task-1".to_owned(), BannerSessionStatus::Completed),
        ]
    );
}
