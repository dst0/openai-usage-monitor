use super::{
    desktop_ipc::DesktopIpc,
    desktop_ipc::IPC_DISCOVERY_TIMEOUT,
    ipc_protocol::{
        ipc_response_wait, parse_owner_info, read_ipc_frame, recovery_turn_start_request,
        validate_start_response, write_ipc_frame, IPC_ROUTER_DISCOVERY_BUDGET, MAX_IPC_FRAME,
    },
    recovery_mode::RecoveryMode,
    target_dispatch::{should_dispatch, should_resume_queued},
};
use crate::switcher::ThreadRolloutState::*;
use serde_json::Value;
use std::{io::Write, os::unix::net::UnixStream, thread, time::Duration};

#[test]
fn desktop_ipc_dispatches_only_eligible_work() {
    for mode in [
        RecoveryMode::CapturedRestart,
        RecoveryMode::DeferredCaptured,
        RecoveryMode::DeferredOwned,
        RecoveryMode::ExplicitTarget,
        RecoveryMode::DiscoveredOnly,
    ] {
        assert_eq!(
            should_dispatch(TurnAborted, 0, mode),
            matches!(
                mode,
                RecoveryMode::CapturedRestart
                    | RecoveryMode::DeferredCaptured
                    | RecoveryMode::ExplicitTarget
            ),
            "discovery-only recovery must not revive an ambiguous historical Stop"
        );
        assert!(should_dispatch(InterruptedByQuota, 0, mode));
        // An error may be a policy block; only an explicit request continues it.
        assert_eq!(
            should_dispatch(InterruptedByError, 0, mode),
            mode == RecoveryMode::ExplicitTarget
        );
        assert!(!should_dispatch(InterruptedByError, 1, mode));
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
        RecoveryMode::DeferredCaptured
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
    assert!(!should_dispatch(
        ActiveInProgress,
        0,
        RecoveryMode::DeferredOwned
    ));
}

#[test]
fn queued_recovery_obeys_turn_and_mode_policy() {
    for mode in [
        RecoveryMode::CapturedRestart,
        RecoveryMode::DeferredCaptured,
        RecoveryMode::DeferredOwned,
        RecoveryMode::ExplicitTarget,
        RecoveryMode::DiscoveredOnly,
    ] {
        assert!(should_resume_queued(InterruptedByQuota, mode));
        assert_eq!(
            should_resume_queued(InterruptedByError, mode),
            mode == RecoveryMode::ExplicitTarget
        );
        assert_eq!(
            should_resume_queued(TurnAborted, mode),
            matches!(
                mode,
                RecoveryMode::CapturedRestart
                    | RecoveryMode::DeferredCaptured
                    | RecoveryMode::ExplicitTarget
            )
        );
        assert_eq!(
            should_resume_queued(CleanCompleted, mode),
            mode != RecoveryMode::DiscoveredOnly
        );
        assert!(!should_resume_queued(Unknown, mode));
    }
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

#[test]
fn separate_task_windows_route_each_turn_to_its_discovered_owner() {
    let first = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
    let second = "01a09c25-a442-78c0-9263-73ae757030e8";
    let (client_stream, mut router_stream) = UnixStream::pair().unwrap();
    let router = thread::spawn(move || {
        let mut requests = Vec::new();
        for _ in 0..4 {
            let request = read_ipc_frame(&mut router_stream).unwrap();
            let method = request["method"].as_str().unwrap().to_string();
            let id = request["params"]["conversationId"]
                .as_str()
                .unwrap()
                .to_string();
            let owner = if id == first {
                "window-one"
            } else {
                "window-two"
            };
            requests.push((
                method.clone(),
                id,
                request["targetClientId"].as_str().map(str::to_string),
            ));
            let result = if method == "thread-owner-discovery" {
                serde_json::json!({"supportsUntrustedAppInput": true})
            } else {
                serde_json::json!({"result": {"turn": {"id": first}}})
            };
            write_ipc_frame(
                &mut router_stream,
                &serde_json::json!({
                    "type": "response",
                    "requestId": request["requestId"],
                    "resultType": "success",
                    "method": method,
                    "handledByClientId": owner,
                    "result": result
                }),
            )
            .unwrap();
        }
        requests
    });
    let mut client = DesktopIpc::for_test(client_stream);
    let first_owner = client.discover_owner_info_once(first).unwrap().client_id;
    let second_owner = client.discover_owner_info_once(second).unwrap().client_id;
    assert_ne!(first_owner, second_owner);
    client.resume_interrupted_turn(first, &first_owner).unwrap();
    client
        .resume_interrupted_turn(second, &second_owner)
        .unwrap();
    let requests = router.join().unwrap();
    assert_eq!(
        requests[0],
        ("thread-owner-discovery".into(), first.into(), None)
    );
    assert_eq!(
        requests[1],
        ("thread-owner-discovery".into(), second.into(), None)
    );
    assert_eq!(
        requests[2],
        (
            "thread-follower-start-turn".into(),
            first.into(),
            Some(first_owner)
        )
    );
    assert_eq!(
        requests[3],
        (
            "thread-follower-start-turn".into(),
            second.into(),
            Some(second_owner)
        )
    );
}
