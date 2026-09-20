use super::{ipc_read_error::IpcReadError, owner_info::OwnerInfo, thread_identity::valid_id};
use serde_json::Value;
use std::{
    io::{Read, Write},
    os::unix::net::UnixStream,
    time::Duration,
};
pub(super) const IPC_ROUTER_DISCOVERY_BUDGET: Duration = Duration::from_secs(10);
const IPC_RESPONSE_GRACE: Duration = Duration::from_secs(2);
pub(super) const MAX_IPC_FRAME: usize = 2 * 1024 * 1024;

pub(super) fn ipc_response_wait(method: &str, timeout: Duration) -> Duration {
    timeout
        .saturating_add(if method == "initialize" {
            Duration::ZERO
        } else {
            IPC_ROUTER_DISCOVERY_BUDGET
        })
        .saturating_add(IPC_RESPONSE_GRACE)
}

pub(super) fn recovery_turn_start_request(thread_id: &str) -> Value {
    serde_json::json!({
        "conversationId": thread_id,
        "turnStart": {
            "request": {
                "threadId": thread_id,
                "input": [{
                    "type": "text",
                    "text": "continue",
                    "text_elements": []
                }],
                "turnTrigger": "app_update_resume"
            },
            "context": { "inheritThreadSettings": true }
        }
    })
}

pub(super) fn parse_owner_info(response: &Value) -> Result<OwnerInfo, String> {
    if response["method"].as_str() != Some("thread-owner-discovery")
        || response["result"]["supportsUntrustedAppInput"].as_bool() != Some(true)
    {
        return Err("The Codex thread owner does not accept local IPC input".into());
    }
    let client_id = response["handledByClientId"]
        .as_str()
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .ok_or_else(|| "Codex Desktop did not report a thread owner".to_string())?;
    Ok(OwnerInfo { client_id })
}

pub(super) fn validate_start_response(response: &Value, owner: &str) -> Result<String, String> {
    if response["method"].as_str() != Some("thread-follower-start-turn") {
        return Err("Codex Desktop returned a mismatched recovery response".into());
    }
    if response["handledByClientId"].as_str() != Some(owner) {
        return Err("Codex Desktop returned recovery from an unexpected owner".into());
    }
    response["result"]["result"]["turn"]["id"]
        .as_str()
        .filter(|id| valid_id(id))
        .map(str::to_string)
        .ok_or_else(|| "Codex Desktop did not return a valid started turn ID".into())
}

pub(super) fn write_ipc_frame(stream: &mut UnixStream, value: &Value) -> Result<(), String> {
    let payload = serde_json::to_vec(value).map_err(|error| error.to_string())?;
    if payload.is_empty() || payload.len() > MAX_IPC_FRAME {
        return Err("Codex IPC request exceeds the recovery memory limit".into());
    }
    stream
        .write_all(&(payload.len() as u32).to_le_bytes())
        .and_then(|()| stream.write_all(&payload))
        .and_then(|()| stream.flush())
        .map_err(|error| error.to_string())
}

pub(super) fn read_ipc_frame(stream: &mut UnixStream) -> Result<Value, IpcReadError> {
    let mut length = [0_u8; 4];
    stream.read_exact(&mut length).map_err(IpcReadError::Io)?;
    let length = u32::from_le_bytes(length) as usize;
    if length == 0 || length > MAX_IPC_FRAME {
        return Err(IpcReadError::Protocol(
            "Codex IPC response exceeds the recovery memory limit".into(),
        ));
    }
    let mut payload = vec![0_u8; length];
    stream.read_exact(&mut payload).map_err(IpcReadError::Io)?;
    serde_json::from_slice(&payload).map_err(|error| IpcReadError::Protocol(error.to_string()))
}
