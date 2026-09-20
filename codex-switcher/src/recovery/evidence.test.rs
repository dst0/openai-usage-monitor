use super::{
    evidence::Evidence,
    recovery_target::{proof_survived_stability_window, RECOVERY_SOAK_WINDOW},
};
use serde_json::Value;
use std::time::{Duration, Instant};
fn event(kind: &str, payload: Value) -> Vec<u8> {
    serde_json::to_vec(&serde_json::json!({"type":kind,"payload":payload,"timestamp":"test"}))
        .unwrap()
}

#[test]
fn only_the_ipc_confirmed_turn_can_satisfy_recovery() {
    let expected = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
    let other = "01a09c25-a442-78c0-9263-73ae757030e8";
    let mut evidence = Evidence::default();
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"task_started","turn_id":other}),
    ));
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    assert!(evidence.verified(Some(other)));
    assert!(!evidence.verified(Some(expected)));
}

#[test]
fn proof_requires_the_full_stability_window() {
    let observed = Instant::now();
    assert!(!proof_survived_stability_window(
        observed,
        observed + RECOVERY_SOAK_WINDOW - Duration::from_millis(1)
    ));
    assert!(proof_survived_stability_window(
        observed,
        observed + RECOVERY_SOAK_WINDOW
    ));
}

#[test]
fn delayed_terminal_for_an_old_turn_does_not_poison_the_replacement() {
    let expected = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
    let old = "01a09c25-a442-78c0-9263-73ae757030e8";
    let mut evidence = Evidence::default();
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"task_started","turn_id":expected}),
    ));
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"turn_aborted","turn_id":old}),
    ));
    assert!(evidence.verified(Some(expected)));
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"turn_aborted","turn_id":expected}),
    ));
    assert!(!evidence.verified(Some(expected)));
}

#[test]
fn queue_start_metadata_and_tool_outputs_are_not_work() {
    let mut evidence = Evidence::default();
    for payload in [
        serde_json::json!({"type":"user_message","message":"continue"}),
        serde_json::json!({"type":"task_started"}),
        serde_json::json!({"type":"token_count"}),
    ] {
        evidence.event(&event("event_msg", payload));
    }
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"function_call_output"}),
    ));
    assert!(!evidence.verified(None));
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    assert!(evidence.verified(None));
    evidence.event(&event("event_msg", serde_json::json!({"type":"task_complete","error":{"codex_error_info":"usage_limit_exceeded"}})));
    assert!(!evidence.verified(None));
}

#[test]
fn expected_restart_abort_before_new_turn_is_not_a_failure() {
    let mut evidence = Evidence::default();
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"turn_aborted"}),
    ));
    assert!(!evidence.failed);
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"task_started"}),
    ));
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    assert!(evidence.verified(None));
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"turn_aborted"}),
    ));
    assert!(evidence.failed);
}

#[test]
fn work_followed_by_restart_abort_is_not_recovery_proof() {
    let mut evidence = Evidence::default();
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    assert!(!evidence.verified(None));
    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"turn_aborted"}),
    ));
    assert!(!evidence.verified(None));
    assert!(evidence.aborted);

    evidence.event(&event(
        "event_msg",
        serde_json::json!({"type":"task_started"}),
    ));
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    assert!(evidence.verified(None));
}

#[test]
fn work_without_a_post_checkpoint_start_is_not_recovery_proof() {
    let mut evidence = Evidence::default();
    evidence.event(&event(
        "response_item",
        serde_json::json!({"type":"reasoning"}),
    ));
    assert!(evidence.work);
    assert!(!evidence.started);
    assert!(!evidence.verified(None));
}
