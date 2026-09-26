use super::{
    ipc_call_error::IpcCallError, ipc_protocol::read_ipc_frame, ipc_read_error::IpcReadError,
};
use serde_json::Value;
use std::{
    io::ErrorKind,
    os::unix::net::UnixStream,
    time::{Duration, Instant},
};

/// Waits on Desktop's IPC connection for the reply to one request.
pub(super) struct IpcResponseReader;

impl IpcResponseReader {
    /// Skips frames that are not the reply to `request_id` (router broadcasts
    /// share the connection) until `deadline`.
    pub(super) fn await_reply(
        stream: &mut UnixStream,
        method: &str,
        request_id: &str,
        deadline: Instant,
    ) -> Result<Value, IpcCallError> {
        loop {
            let remaining = deadline.saturating_duration_since(Instant::now());
            if remaining.is_zero() {
                return Err(Self::timed_out(method));
            }
            Self::arm_read_timeout(stream, remaining)?;
            let response = match read_ipc_frame(stream) {
                Ok(response) => response,
                Err(IpcReadError::Io(error))
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) =>
                {
                    return Err(Self::timed_out(method));
                }
                Err(error) => return Err(IpcCallError::Other(error.to_string())),
            };
            if response["type"].as_str() != Some("response")
                || response["requestId"].as_str() != Some(request_id)
            {
                continue;
            }
            if response["resultType"].as_str() == Some("error") {
                let error = response["error"].as_str().unwrap_or("unknown error");
                return Err(if error == "no-client-found" {
                    IpcCallError::NoClientFound
                } else {
                    IpcCallError::Other(format!("Codex IPC rejected {method}: {error}"))
                });
            }
            if response["resultType"].as_str() != Some("success") {
                return Err(IpcCallError::Other(format!(
                    "Codex IPC returned an invalid response for {method}"
                )));
            }
            return Ok(response);
        }
    }

    /// XNU rejects `setsockopt` with `EINVAL` once the router has closed its
    /// end, even while the router's complete reply is still buffered. Such a
    /// socket cannot block: a read returns the buffered bytes, then EOF, so
    /// only that kernel error is tolerated. Everything else, including Rust's
    /// own refusal of a zero timeout, would leave the read unbounded.
    fn arm_read_timeout(stream: &UnixStream, timeout: Duration) -> Result<(), IpcCallError> {
        match stream.set_read_timeout(Some(timeout)) {
            Err(error) if error.raw_os_error() != Some(libc::EINVAL) => {
                Err(IpcCallError::Other(error.to_string()))
            }
            _ => Ok(()),
        }
    }

    fn timed_out(method: &str) -> IpcCallError {
        IpcCallError::Other(format!("Codex IPC request {method} timed out"))
    }
}

#[cfg(test)]
#[path = "ipc_response_reader.test.rs"]
mod tests;
