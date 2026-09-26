use super::{inspect_thread_rollout_state, inspect_thread_rollout_state_from_lines};
use crate::switcher::ThreadRolloutState;

fn lines(raw: &[&str]) -> Vec<String> {
    raw.iter().map(|line| line.to_string()).collect()
}

/// Shape of the 2026-09-26 Codex outage: every model request returned 401,
/// Desktop closed the turn with an error and no final agent message, then
/// later applied thread settings when the task was reopened.
const OUTAGE_401_TASK_COMPLETE: &str = r#"{"timestamp":"2026-09-25T23:26:49.406Z","type":"event_msg","payload":{"type":"task_complete","turn_id":"turn-401","last_agent_message":null,"error":{"message":"unexpected status 401 Unauthorized: Incorrect API key provided: <redacted>. You can find your API key at https://platform.openai.com/account/api-keys., url: https://chatgpt.com/backend-api/codex/responses","codex_error_info":"other"},"started_at":1,"completed_at":2,"duration_ms":1}}"#;

#[test]
fn outage_401_without_final_message_is_interrupted_not_completed() {
    let state = inspect_thread_rollout_state_from_lines(&lines(&[
        r#"{"type":"response_item","payload":{"type":"message","role":"user","content":[{"type":"input_text","text":"so?"}]}}"#,
        r#"{"type":"event_msg","payload":{"type":"token_count","rate_limits":{"primary":{"used_percent":49.0}}}}"#,
        OUTAGE_401_TASK_COMPLETE,
        r#"{"type":"event_msg","payload":{"type":"thread_settings_applied","thread_id":"th-1"}}"#,
    ]));
    assert_eq!(state, ThreadRolloutState::InterruptedByError);
}

#[test]
fn unauthorized_refresh_error_with_absent_final_message_is_interrupted() {
    let state = inspect_thread_rollout_state_from_lines(&lines(&[
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"continue"}}"#,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","error":{"message":"Your access token could not be refreshed because you have since logged out or signed in to another account.","codex_error_info":"unauthorized"}}}"#,
    ]));
    assert_eq!(state, ThreadRolloutState::InterruptedByError);
}

#[test]
fn error_with_blank_final_message_is_interrupted() {
    let state = inspect_thread_rollout_state_from_lines(&lines(&[
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":"  \n","error":{"message":"stream disconnected before completion","codex_error_info":"other"}}}"#,
    ]));
    assert_eq!(state, ThreadRolloutState::InterruptedByError);
}

#[test]
fn error_after_a_final_agent_message_stays_completed() {
    let state = inspect_thread_rollout_state_from_lines(&lines(&[
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":"Done: tests pass.","error":{"message":"post-turn hook failed","codex_error_info":"other"}}}"#,
    ]));
    assert_eq!(state, ThreadRolloutState::CleanCompleted);
}

#[test]
fn successful_turn_without_final_message_stays_completed() {
    // A null error is success even when the turn only ran tools.
    for payload in [
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":null,"error":null}}"#,
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":null}}"#,
    ] {
        assert_eq!(
            inspect_thread_rollout_state_from_lines(&lines(&[payload])),
            ThreadRolloutState::CleanCompleted
        );
    }
}

#[test]
fn quota_error_without_final_message_keeps_quota_classification() {
    let state = inspect_thread_rollout_state_from_lines(&lines(&[
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t","last_agent_message":null,"error":{"message":"Your workspace is out of credits. Add credits to continue.","codex_error_info":"usage_limit_exceeded"}}}"#,
    ]));
    assert_eq!(state, ThreadRolloutState::InterruptedByQuota);
}

#[test]
fn non_string_final_message_fails_closed_as_completed() {
    for message in [r#"{"text":"done"}"#, "42", "true", r#"["done"]"#] {
        let line = format!(
            r#"{{"type":"event_msg","payload":{{"type":"task_complete","turn_id":"t","last_agent_message":{message},"error":{{"message":"stream disconnected","codex_error_info":"other"}}}}}}"#
        );
        assert_eq!(
            inspect_thread_rollout_state_from_lines(&[line]),
            ThreadRolloutState::CleanCompleted,
            "last_agent_message={message}"
        );
    }
}

#[test]
fn later_turn_events_override_an_earlier_error() {
    // A user Stop after the failed turn stays an ambiguous abort.
    assert_eq!(
        inspect_thread_rollout_state_from_lines(&lines(&[
            OUTAGE_401_TASK_COMPLETE,
            r#"{"type":"event_msg","payload":{"type":"turn_aborted","reason":"interrupted"}}"#,
        ])),
        ThreadRolloutState::TurnAborted
    );
    // A new user turn after the failure is live work, not the failed turn.
    assert_eq!(
        inspect_thread_rollout_state_from_lines(&lines(&[
            OUTAGE_401_TASK_COMPLETE,
            r#"{"type":"event_msg","payload":{"type":"user_message","message":"continue"}}"#,
        ])),
        ThreadRolloutState::ActiveInProgress
    );
    // Unrecognized trailing records do not hide the terminal error.
    assert_eq!(
        inspect_thread_rollout_state_from_lines(&lines(&[
            OUTAGE_401_TASK_COMPLETE,
            r#"{"type":"world_state","payload":{"full":false}}"#,
            r#"{"type":"event_msg","payload":{"type":"some_future_event"}}"#,
            "{truncated",
        ])),
        ThreadRolloutState::InterruptedByError
    );
}

#[test]
fn rollout_file_ending_in_outage_401_is_interrupted() {
    let root = std::env::temp_dir().join(format!(
        "codex-rollout-outage-401-{}-{}",
        std::process::id(),
        chrono::Utc::now().timestamp_nanos_opt().unwrap_or_default()
    ));
    let sessions = root.join("sessions").join("2026").join("09").join("25");
    std::fs::create_dir_all(&sessions).unwrap();
    let tid = "01a0d8a1-f49c-7cd1-8e5f-cbf327837b5d";
    let rollout = sessions.join(format!("rollout-2026-09-25T22-54-47-{tid}.jsonl"));
    let content = [
        r#"{"type":"event_msg","payload":{"type":"task_complete","turn_id":"t0","last_agent_message":"earlier work finished","error":null}}"#,
        r#"{"type":"event_msg","payload":{"type":"user_message","message":"so?"}}"#,
        OUTAGE_401_TASK_COMPLETE,
    ]
    .join("\n");
    std::fs::write(&rollout, content + "\n").unwrap();

    let state = inspect_thread_rollout_state(&root, tid);

    std::fs::remove_dir_all(&root).unwrap();
    assert_eq!(state, ThreadRolloutState::InterruptedByError);
}
