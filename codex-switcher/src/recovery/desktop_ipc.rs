use super::{
    ipc_call_error::IpcCallError,
    ipc_protocol::{
        ipc_response_wait, parse_owner_info, recovery_turn_start_request, validate_start_response,
        write_ipc_frame,
    },
    ipc_response_reader::IpcResponseReader,
    owner_info::OwnerInfo,
    owner_link_retry_schedule::{self, OwnerLinkRetry},
    ownerless_link_mount,
};
use crate::switcher;
use serde_json::Value;
use std::{
    os::unix::net::UnixStream,
    thread::sleep,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};
const IPC_CALL_TIMEOUT: Duration = Duration::from_secs(60);
const IPC_OWNER_TIMEOUT: Duration = Duration::from_secs(90);
pub(super) const IPC_DISCOVERY_TIMEOUT: Duration = Duration::from_secs(12);

/// Desktop's same-user IPC router addresses the owner of a mounted thread.
pub(super) struct DesktopIpc {
    stream: UnixStream,
    client_id: String,
    next_id: u64,
}

impl DesktopIpc {
    #[cfg(test)]
    pub(super) fn for_test(stream: UnixStream) -> Self {
        Self {
            stream,
            client_id: "test-monitor".into(),
            next_id: 1,
        }
    }

    pub(super) fn connect_with_retry(timeout: Duration) -> Result<Self, String> {
        let deadline = Instant::now() + timeout;
        loop {
            let last_error = match Self::connect(Duration::from_secs(5)) {
                Ok(client) => return Ok(client),
                Err(error) => error,
            };
            if Instant::now() >= deadline {
                return Err(format!(
                    "Codex Desktop IPC did not become ready within {}s ({})",
                    timeout.as_secs(),
                    last_error
                ));
            }
            sleep(Duration::from_millis(200));
        }
    }

    pub(super) fn connect(timeout: Duration) -> Result<Self, String> {
        let stream = super::ipc_socket::connect_verified_socket()?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        stream
            .set_write_timeout(Some(timeout))
            .map_err(|error| error.to_string())?;
        let mut client = Self {
            stream,
            client_id: "initializing-client".into(),
            next_id: 1,
        };
        let initialized = client
            .request_raw(
                "initialize",
                0,
                serde_json::json!({ "clientType": "codex-monitor" }),
                None,
                timeout,
            )
            .map_err(|error| error.to_string())?;
        client.client_id = initialized["result"]["clientId"]
            .as_str()
            .filter(|id| !id.is_empty())
            .ok_or("Codex IPC initialize returned no client ID")?
            .to_string();
        Ok(client)
    }

    pub(super) fn request_raw(
        &mut self,
        method: &str,
        version: u64,
        params: Value,
        target_client_id: Option<&str>,
        timeout: Duration,
    ) -> Result<Value, IpcCallError> {
        let deadline = Instant::now() + ipc_response_wait(method, timeout);
        let request_id = format!(
            "codex-monitor-{}-{}-{}",
            std::process::id(),
            self.next_id,
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_nanos()
        );
        self.next_id += 1;
        let mut request = serde_json::json!({
            "type": "request",
            "requestId": request_id,
            "sourceClientId": self.client_id,
            "version": version,
            "method": method,
            "params": params,
            "timeoutMs": timeout.as_millis().min(u64::MAX as u128) as u64
        });
        if let Some(target) = target_client_id {
            request["targetClientId"] = Value::String(target.to_string());
        }
        write_ipc_frame(&mut self.stream, &request).map_err(IpcCallError::Other)?;
        IpcResponseReader::await_reply(&mut self.stream, method, &request_id, deadline)
    }

    pub(super) fn discover_owner_info_once(
        &mut self,
        thread_id: &str,
    ) -> Result<OwnerInfo, IpcCallError> {
        let response = self.request_raw(
            "thread-owner-discovery",
            1,
            serde_json::json!({ "hostId": "local", "conversationId": thread_id }),
            None,
            IPC_DISCOVERY_TIMEOUT,
        )?;
        parse_owner_info(&response).map_err(IpcCallError::Other)
    }

    pub(super) fn discover_owner_info_with_retry(
        &mut self,
        thread_id: &str,
    ) -> Result<OwnerInfo, IpcCallError> {
        let started = Instant::now();
        let deadline = started + IPC_OWNER_TIMEOUT;
        let mut last_ordinary = started;
        let mut native_attempted = false;
        loop {
            match self.discover_owner_info_once(thread_id) {
                Ok(owner) => return Ok(owner),
                Err(IpcCallError::NoClientFound) => {}
                Err(error) => return Err(error),
            }
            if Instant::now() >= deadline {
                return Err(IpcCallError::NoClientFound);
            }
            sleep(Duration::from_millis(200));
            let now = Instant::now();
            if let Some(selected) = owner_link_retry_schedule::route(
                now.duration_since(started),
                now.duration_since(last_ordinary),
                native_attempted,
            ) {
                match selected {
                    OwnerLinkRetry::PinnedNative => native_attempted = true,
                    OwnerLinkRetry::Ordinary => last_ordinary = now,
                }
                if let Err(error) =
                    owner_link_retry_schedule::retry_thread_link(thread_id, selected)
                {
                    if switcher::is_fatal_thread_navigation_error(&error) {
                        return Err(IpcCallError::Other(error));
                    }
                    let reason = match selected {
                        OwnerLinkRetry::PinnedNative => "PINNED_TASK_NAVIGATION_FAILED",
                        OwnerLinkRetry::Ordinary => "TASK_NAVIGATION_RETRY_FAILED",
                    };
                    crate::logger::log("WARN", "RECOVERY", reason);
                }
            }
        }
    }

    pub(super) fn ensure_thread_owner(
        &mut self,
        thread_id: &str,
    ) -> Result<(String, bool), IpcCallError> {
        let (owner, mounted_by_recovery) = self.ensure_thread_owner_info(thread_id)?;
        Ok((owner.client_id, mounted_by_recovery))
    }

    pub(super) fn ensure_thread_owner_info(
        &mut self,
        thread_id: &str,
    ) -> Result<(OwnerInfo, bool), IpcCallError> {
        // An already visible task may have an owner immediately. A cold task
        // has none until its deep link mounts the Desktop view asynchronously.
        let (owner, mounted_by_recovery) = match self.discover_owner_info_once(thread_id) {
            Ok(owner) => (owner, false),
            Err(IpcCallError::NoClientFound) => {
                let owner = ownerless_link_mount::open_then_wait_for_owner(
                    || switcher::open_thread_in_codex(thread_id),
                    || self.discover_owner_info_with_retry(thread_id),
                )?;
                (owner, true)
            }
            Err(error) => return Err(error),
        };
        Ok((owner, mounted_by_recovery))
    }

    pub(super) fn resume_interrupted_turn(
        &mut self,
        thread_id: &str,
        owner: &str,
    ) -> Result<String, String> {
        let response = self
            .request_raw(
                "thread-follower-start-turn",
                2,
                recovery_turn_start_request(thread_id),
                Some(owner),
                IPC_CALL_TIMEOUT,
            )
            .map_err(|error| error.to_string())?;
        let turn_id = validate_start_response(&response, owner)?;
        Ok(turn_id)
    }

    pub(super) fn resume_existing_queue(
        &mut self,
        thread_id: &str,
        messages: Vec<Value>,
        owner: &str,
    ) -> Result<(), String> {
        let mut state = serde_json::Map::new();
        state.insert(thread_id.to_string(), Value::Array(messages));
        let response = self
            .request_raw(
                "thread-follower-set-queued-follow-ups-state",
                1,
                serde_json::json!({
                    "conversationId": thread_id,
                    "state": state
                }),
                Some(owner),
                IPC_CALL_TIMEOUT,
            )
            .map_err(|error| error.to_string())?;
        if response["method"].as_str() != Some("thread-follower-set-queued-follow-ups-state")
            || response["handledByClientId"].as_str() != Some(owner)
            || response["result"]["ok"].as_bool() != Some(true)
        {
            return Err("Codex Desktop did not confirm queue recovery".into());
        }
        Ok(())
    }
}
