use super::{
    desktop_ipc::IPC_DISCOVERY_TIMEOUT,
    ipc_protocol::{
        ipc_response_wait, parse_owner_info, read_ipc_frame, recovery_turn_start_request,
        validate_start_response, write_ipc_frame, IPC_ROUTER_DISCOVERY_BUDGET, MAX_IPC_FRAME,
    },
    recovery_mode::RecoveryMode,
    target_dispatch::should_dispatch,
};
use crate::switcher::ThreadRolloutState::*;
use serde_json::Value;
use std::{io::Write, os::unix::net::UnixStream, time::Duration};

#[test]
fn desktop_ipc_dispatches_only_eligible_work() {
    for mode in [
        RecoveryMode::CapturedRestart,
        RecoveryMode::ExplicitTarget,
        RecoveryMode::DiscoveredOnly,
    ] {
        assert!(should_dispatch(TurnAborted, 0, mode));
        assert!(should_dispatch(InterruptedByQuota, 0, mode));
        assert!(!should_dispatch(InterruptedByQuota, 1, mode));
        assert!(!should_dispatch(CleanCompleted, 0, mode));
        assert!(!should_dispatch(Unknown, 0, mode));
    }
    assert!(should_dispatch(
        ActiveInProgress,
        0,
        RecoveryMode::CapturedRestart
    ));
    assert!(should_dispatch(
        ActiveInProgress,
        0,
        RecoveryMode::ExplicitTarget
    ));
    assert!(!should_dispatch(
        ActiveInProgress,
        0,
        RecoveryMode::DiscoveredOnly
    ));
}

#[test]
fn ipc_framing_round_trips_and_rejects_invalid_lengths() {
    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    let value = serde_json::json!({"type":"request","method":"initialize"});
    write_ipc_frame(&mut writer, &value).unwrap();
    assert_eq!(read_ipc_frame(&mut reader).unwrap(), value);

    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    writer.write_all(&0_u32.to_le_bytes()).unwrap();
    assert!(read_ipc_frame(&mut reader).is_err());

    let (mut writer, mut reader) = UnixStream::pair().unwrap();
    writer
        .write_all(&((MAX_IPC_FRAME + 1) as u32).to_le_bytes())
        .unwrap();
    assert!(read_ipc_frame(&mut reader).is_err());
}

#[test]
fn ipc_transport_outlives_the_router_timeout() {
    assert_eq!(
        ipc_response_wait("initialize", Duration::from_secs(5)),
        Duration::from_secs(7)
    );
    assert_eq!(
        ipc_response_wait("thread-owner-discovery", IPC_DISCOVERY_TIMEOUT),
        Duration::from_secs(24)
    );
    assert!(IPC_DISCOVERY_TIMEOUT > IPC_ROUTER_DISCOVERY_BUDGET);
}

#[test]
fn recovery_turn_uses_one_protocol_valid_continue_text_input() {
    let thread_id = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
    let request = recovery_turn_start_request(thread_id);
    assert_eq!(request["conversationId"], thread_id);
    assert_eq!(request["turnStart"]["request"]["threadId"], thread_id);
    assert_eq!(
        request["turnStart"]["request"]["input"],
        serde_json::json!([{
            "type": "text",
            "text": "continue",
            "text_elements": []
        }])
    );
    assert_eq!(
        request["turnStart"]["request"]["turnTrigger"],
        "app_update_resume"
    );
    assert_eq!(
        request["turnStart"]["context"]["inheritThreadSettings"],
        true
    );
}

#[test]
fn owner_and_start_responses_are_bound_to_the_expected_owner() {
    let owner_response = serde_json::json!({
        "method": "thread-owner-discovery",
        "handledByClientId": "owner-1",
        "result": { "supportsUntrustedAppInput": true }
    });
    assert_eq!(
        parse_owner_info(&owner_response).unwrap().client_id,
        "owner-1"
    );
    let mut rejected_owner = owner_response.clone();
    rejected_owner["result"]["supportsUntrustedAppInput"] = Value::Bool(false);
    assert!(parse_owner_info(&rejected_owner).is_err());

    let start_response = serde_json::json!({
        "method": "thread-follower-start-turn",
        "handledByClientId": "owner-1",
        "result": { "result": { "turn": { "id": "01a09c25-9480-7dc2-87fd-79c507f91fcb" } } }
    });
    assert_eq!(
        validate_start_response(&start_response, "owner-1").unwrap(),
        "01a09c25-9480-7dc2-87fd-79c507f91fcb"
    );
    assert!(validate_start_response(&start_response, "owner-2").is_err());
    let mut missing_turn = start_response.clone();
    missing_turn["result"]["result"]["turn"] = Value::Null;
    assert!(validate_start_response(&missing_turn, "owner-1").is_err());
    let mut malformed_turn = start_response;
    malformed_turn["result"]["result"]["turn"]["id"] = Value::String("turn\n1".into());
    assert!(validate_start_response(&malformed_turn, "owner-1").is_err());
}
