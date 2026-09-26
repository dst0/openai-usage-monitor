use super::{
    desktop_ipc::DesktopIpc,
    ipc_protocol::{write_ipc_frame, MAX_IPC_FRAME},
};
use serde_json::Value;
use std::{
    io::{ErrorKind, Read},
    os::unix::net::UnixStream,
    thread::{self, JoinHandle},
};

/// A scripted stand-in for Codex Desktop's same-user IPC router.
///
/// It answers each request with the scripted reply and keeps its end of the
/// connection open until the client hangs up, like the real router. A request
/// the client never sends is observed as the client's EOF in [`Self::finish`],
/// never inferred from a read timeout: recovery does SQLite, manifest, and
/// banner work between two requests, and a loaded machine can outlast any
/// fixed timeout. Closing early would also turn a slow client's next write
/// into `EPIPE` and hide what it tried to send.
pub(super) struct TestDesktopRouter {
    worker: JoinHandle<Vec<Value>>,
}

impl TestDesktopRouter {
    /// `reply` returns the response frame for each request. It runs on the
    /// router thread, so it can also change test state between two requests
    /// exactly as a concurrent Desktop would.
    pub(super) fn start(
        mut reply: impl FnMut(&Value) -> Value + Send + 'static,
    ) -> (DesktopIpc, Self) {
        let (client, mut router) = UnixStream::pair().unwrap();
        let worker = thread::spawn(move || {
            let mut requests = Vec::new();
            while let Some(request) = Self::next_request(&mut router) {
                let response = reply(&request);
                requests.push(request);
                write_ipc_frame(&mut router, &response)
                    .expect("client hung up before reading its response");
            }
            requests
        });
        (DesktopIpc::for_test(client), Self { worker })
    }

    /// The next request frame, or `None` once the client has hung up between
    /// frames. A frame cut short is a client defect and fails the test.
    fn next_request(router: &mut UnixStream) -> Option<Value> {
        let mut length = [0_u8; 4];
        loop {
            match router.read(&mut length[..1]) {
                Ok(0) => return None,
                Ok(_) => break,
                Err(error) if error.kind() == ErrorKind::Interrupted => {}
                Err(error) => panic!("router read failed: {error}"),
            }
        }
        router
            .read_exact(&mut length[1..])
            .expect("client sent a truncated IPC frame");
        let length = u32::from_le_bytes(length) as usize;
        assert!(
            (1..=MAX_IPC_FRAME).contains(&length),
            "client sent an IPC frame of {length} bytes"
        );
        let mut payload = vec![0_u8; length];
        router
            .read_exact(&mut payload)
            .expect("client sent a truncated IPC frame");
        Some(serde_json::from_slice(&payload).expect("client sent an unreadable IPC frame"))
    }

    /// Hangs up `client`, then returns every request the router received, in
    /// order. A panic on the router thread, such as a failed assertion inside
    /// `reply`, is re-raised here.
    pub(super) fn finish(self, client: DesktopIpc) -> Vec<Value> {
        drop(client);
        self.worker
            .join()
            .unwrap_or_else(|panic| std::panic::resume_unwind(panic))
    }

    /// A successful response to `request`, handled by the `owner` window.
    pub(super) fn success(request: &Value, owner: &str, result: Value) -> Value {
        serde_json::json!({
            "type": "response",
            "requestId": request["requestId"],
            "resultType": "success",
            "method": request["method"],
            "handledByClientId": owner,
            "result": result
        })
    }
}
