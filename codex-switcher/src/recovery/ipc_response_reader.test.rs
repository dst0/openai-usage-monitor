use super::IpcResponseReader;
use crate::recovery::{
    desktop_ipc::DesktopIpc,
    ipc_call_error::IpcCallError,
    ipc_protocol::{read_ipc_frame, write_ipc_frame},
    test_desktop_router::TestDesktopRouter,
};
use serde_json::{json, Value};
use std::{
    os::unix::net::UnixStream,
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

const METHOD: &str = "thread-follower-start-turn";
const REQUEST: &str = "codex-monitor-1-1-1";

fn reply(request_id: &str, result_type: &str) -> Value {
    json!({
        "type": "response",
        "requestId": request_id,
        "resultType": result_type,
        "method": METHOD,
        "handledByClientId": "window-one",
        "result": {"result": {"turn": {"id": "01a09c25-9480-7dc2-87fd-79c507f91fcb"}}}
    })
}

/// Queues `frames` on a connection whose router has already hung up.
fn router_that_replied_and_closed(frames: &[Value]) -> UnixStream {
    let (client, mut router) = UnixStream::pair().unwrap();
    for frame in frames {
        write_ipc_frame(&mut router, frame).unwrap();
    }
    drop(router);
    client
}

fn await_reply(client: &mut UnixStream) -> Result<Value, IpcCallError> {
    IpcResponseReader::await_reply(
        client,
        METHOD,
        REQUEST,
        Instant::now() + Duration::from_secs(5),
    )
}

#[test]
fn reply_buffered_before_the_router_hung_up_is_not_lost() {
    // XNU rejects setsockopt with EINVAL once the peer has closed, even though
    // its complete reply is still readable. Desktop may answer and exit.
    let mut client = router_that_replied_and_closed(&[reply(REQUEST, "success")]);

    let response = await_reply(&mut client).expect("the buffered reply must be read");

    assert_eq!(response["requestId"], REQUEST);
    assert_eq!(response["handledByClientId"], "window-one");
}

#[test]
fn frames_for_other_requests_are_skipped_until_the_reply() {
    let broadcast = json!({"type": "broadcast", "method": "thread-stream-state-changed"});
    let mut client = router_that_replied_and_closed(&[
        broadcast,
        reply("codex-monitor-1-0-0", "error"),
        reply(REQUEST, "success"),
    ]);

    assert_eq!(await_reply(&mut client).unwrap()["requestId"], REQUEST);
}

#[test]
fn no_client_found_stays_distinct_from_other_rejections() {
    let mut ownerless = reply(REQUEST, "error");
    ownerless["error"] = json!("no-client-found");
    let mut rejected = reply(REQUEST, "error");
    rejected["error"] = json!("thread-not-loaded");

    let ownerless = await_reply(&mut router_that_replied_and_closed(&[ownerless]));
    let rejected = await_reply(&mut router_that_replied_and_closed(&[rejected]));

    assert!(matches!(ownerless, Err(IpcCallError::NoClientFound)));
    assert!(matches!(
        rejected,
        Err(IpcCallError::Other(ref message))
            if message == &format!("Codex IPC rejected {METHOD}: thread-not-loaded")
    ));
}

#[test]
fn unknown_result_type_is_rejected() {
    let mut client = router_that_replied_and_closed(&[reply(REQUEST, "pending")]);

    let error = await_reply(&mut client).unwrap_err();

    assert_eq!(
        error.to_string(),
        format!("Codex IPC returned an invalid response for {METHOD}")
    );
}

#[test]
fn hang_up_without_a_reply_is_an_error_not_a_timeout() {
    let mut client = router_that_replied_and_closed(&[]);

    let error = await_reply(&mut client).unwrap_err();

    assert!(matches!(error, IpcCallError::Other(_)));
    assert!(!error.to_string().contains("timed out"), "{error}");
}

#[test]
fn a_silent_router_is_bounded_by_the_deadline() {
    // The reader runs on a worker, so a regression that leaves the read
    // unbounded fails this test instead of hanging the suite.
    let (mut client, router) = UnixStream::pair().unwrap();
    let (sender, receiver) = mpsc::channel();
    thread::spawn(move || {
        let deadline = Instant::now() + Duration::from_millis(100);
        let result = IpcResponseReader::await_reply(&mut client, METHOD, REQUEST, deadline);
        sender
            .send(result.map_err(|error| error.to_string()))
            .unwrap();
    });

    let result = receiver
        .recv_timeout(Duration::from_secs(10))
        .expect("the reader outlived its deadline");

    drop(router);
    assert_eq!(
        result.unwrap_err(),
        format!("Codex IPC request {METHOD} timed out")
    );
}

#[test]
fn an_expired_deadline_does_not_read_a_waiting_reply() {
    let (mut client, mut router) = UnixStream::pair().unwrap();
    write_ipc_frame(&mut router, &reply(REQUEST, "success")).unwrap();

    let error =
        IpcResponseReader::await_reply(&mut client, METHOD, REQUEST, Instant::now()).unwrap_err();

    assert!(error.to_string().contains("timed out"), "{error}");
}

#[test]
fn arming_tolerates_only_the_kernel_refusal_after_the_router_hangs_up() {
    let (connected, _router) = UnixStream::pair().unwrap();
    IpcResponseReader::arm_read_timeout(&connected, Duration::from_secs(7)).unwrap();
    assert_eq!(
        connected.read_timeout().unwrap(),
        Some(Duration::from_secs(7))
    );
    // Rust refuses a zero timeout as InvalidInput with no OS error. Accepting
    // that would leave the read unbounded, so it must remain an error.
    assert!(IpcResponseReader::arm_read_timeout(&connected, Duration::ZERO).is_err());

    let (closed, router) = UnixStream::pair().unwrap();
    drop(router);
    let refusal = closed
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap_err();
    assert_eq!(refusal.raw_os_error(), Some(libc::EINVAL));
    IpcResponseReader::arm_read_timeout(&closed, Duration::from_secs(7)).unwrap();
}

#[test]
fn desktop_that_answers_and_exits_still_yields_the_started_turn() {
    // Covers the DesktopIpc request path end to end. The router sends an
    // unrelated frame larger than the socket buffer, so that write returns
    // only once the client has drained most of it; the client then spends far
    // longer parsing it than the router needs to answer and exit. The client
    // therefore re-arms its read timeout after the hang-up.
    let turn = "01a09c25-9480-7dc2-87fd-79c507f91fcb";
    for _ in 0..5 {
        let (client, mut router) = UnixStream::pair().unwrap();
        let exiting_desktop = thread::spawn(move || {
            let request = read_ipc_frame(&mut router).unwrap();
            let broadcast = json!({"type": "broadcast", "padding": "x".repeat(1 << 20)});
            write_ipc_frame(&mut router, &broadcast).unwrap();
            let result = json!({"result": {"turn": {"id": turn}}});
            write_ipc_frame(
                &mut router,
                &TestDesktopRouter::success(&request, "window-one", result),
            )
            .unwrap();
        });
        let mut desktop = DesktopIpc::for_test(client);

        let started = desktop.resume_interrupted_turn(turn, "window-one");

        // Hang up first: a client that never sent must fail, not deadlock.
        drop(desktop);
        exiting_desktop.join().unwrap();
        assert_eq!(started.unwrap(), turn);
    }
}
