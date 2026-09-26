use super::*;
use crate::distribution::mock_app_lifecycle::MockAppLifecycle;

#[test]
fn process_inspection_failure_blocks_both_distribution_guards() {
    let lifecycle = MockAppLifecycle::new(false);
    lifecycle.set_running_probe_error("ps failed");
    let plan = DistributionPlan::no_action(None, None, "idle", Vec::new());

    assert!(DistributionSharedAuthGuard::before_journal(&lifecycle, &plan).is_err());
    assert!(DistributionSharedAuthGuard::require_desktop_stopped(&lifecycle).is_err());
}

#[test]
fn offline_app_cli_split_is_rejected_before_any_shared_auth_change() {
    let lifecycle = MockAppLifecycle::new(false);
    let plan = DistributionPlan {
        current_app_id: Some("account-a".into()),
        current_cli_id: Some("account-a".into()),
        target_app_id: Some("account-a".into()),
        target_cli_id: Some("account-b".into()),
        app_switch_needed: false,
        cli_switch_needed: true,
        restart_required: false,
        decision_reason: "explicit split".into(),
        evaluated_candidates: Vec::new(),
    };

    assert!(
        DistributionSharedAuthGuard::before_journal(&lifecycle, &plan)
            .unwrap_err()
            .contains("different target accounts")
    );
}

#[test]
fn offline_shared_auth_change_requires_both_targets() {
    let lifecycle = MockAppLifecycle::new(false);
    let mut plan = DistributionPlan::no_action(None, None, "offline", Vec::new());
    plan.target_cli_id = Some("account-b".into());
    plan.cli_switch_needed = true;

    assert!(DistributionSharedAuthGuard::before_journal(&lifecycle, &plan).is_err());
    plan.target_app_id = Some("account-b".into());
    assert!(DistributionSharedAuthGuard::before_journal(&lifecycle, &plan).is_ok());
}
