use super::distribution_coordinator::DistributionCoordinator;
use super::distribution_request::DistributionRequest;
use super::mock_app_lifecycle::MockAppLifecycle;
use super::test_helper::{make_account, TestEnv};
use crate::storage::{load_accounts, read_active_auth_json};
use std::os::unix::fs::PermissionsExt;
use std::sync::atomic::Ordering;
use std::sync::Arc;

#[test]
fn malformed_prior_direct_switch_intent_blocks_distribution_without_touching_auth() {
    let _guard = crate::setup::TEST_CODEX_HOME_MUTEX
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let env = TestEnv::new("direct_switch_intent_gate");
    let prior = make_account(
        "prior",
        None,
        "prior@example.com",
        "pro",
        5.0,
        Some(70.0),
        0,
        None,
        None,
    );
    let target = make_account(
        "target",
        None,
        "target@example.com",
        "team",
        95.0,
        Some(80.0),
        0,
        None,
        None,
    );
    env.populate(vec![prior, target], Some("prior"), Some("prior"));
    let original_auth = read_active_auth_json().unwrap();
    let original_registry = load_accounts().unwrap();
    let journal_path = env.home().join("direct-switch-journal.json");
    std::fs::write(&journal_path, b"{invalid direct switch intent").unwrap();
    std::fs::set_permissions(&journal_path, std::fs::Permissions::from_mode(0o600)).unwrap();

    let lifecycle = Arc::new(MockAppLifecycle::new(false));
    let coordinator = DistributionCoordinator::with_lifecycle(lifecycle.clone());
    let request = DistributionRequest::user("synthetic recovery guard")
        .with_preferred_app(Some("target".into()))
        .with_preferred_cli(Some("target".into()));
    let result = coordinator.execute(request);

    assert!(
        result.is_err(),
        "unresolved direct intent must block distribution"
    );
    assert_eq!(read_active_auth_json().unwrap(), original_auth);
    assert_eq!(
        serde_json::to_value(load_accounts().unwrap()).unwrap(),
        serde_json::to_value(original_registry).unwrap()
    );
    assert_eq!(lifecycle.stop_calls.load(Ordering::SeqCst), 0);
    assert!(
        journal_path.exists(),
        "ambiguous intent must remain for review"
    );
}
