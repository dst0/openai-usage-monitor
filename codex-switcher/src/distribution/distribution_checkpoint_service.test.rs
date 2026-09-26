use super::*;
use crate::distribution::test_helper::TestEnv;
use serde_json::{json, Value};
use std::sync::atomic::Ordering;

fn prior_manifest() -> Value {
    json!({"version": 1, "targets": [{
        "id": "01a098c2-0fae-74d2-a80c-45d89e910e79", "offset": 42,
        "awaiting_owner": false, "captured_restart": true, "owner_account_id": null
    }]})
}

fn read_manifest(env: &TestEnv) -> Value {
    serde_json::from_slice(&std::fs::read(env.home().join("desktop-recovery.json")).unwrap())
        .unwrap()
}

#[test]
fn window_rejection_before_save_leaves_prior_journal_untouched() {
    let env = TestEnv::new("window_first_preflight");
    let target = prior_manifest();
    std::fs::write(env.home().join("desktop-recovery.json"), target.to_string()).unwrap();
    let mock = MockAppLifecycle::new(true);
    mock.set_preflight_error_on_call(1, "two windows");
    assert!(DistributionCheckpointService::prepare(env.home(), &mock, &[]).is_err());
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 1);
    assert_eq!(read_manifest(&env), target);
}

#[test]
fn window_rejection_after_save_restores_prior_journal() {
    let env = TestEnv::new("window_second_preflight");
    let target = prior_manifest();
    std::fs::write(env.home().join("desktop-recovery.json"), target.to_string()).unwrap();
    let mock = MockAppLifecycle::new(true);
    mock.set_preflight_error_on_call(2, "two windows");
    assert!(DistributionCheckpointService::prepare(env.home(), &mock, &[]).is_err());
    assert_eq!(mock.preflight_calls.load(Ordering::SeqCst), 2);
    assert_eq!(read_manifest(&env), target);
}
